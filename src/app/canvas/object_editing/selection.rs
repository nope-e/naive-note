//! Page-bound region selection/rotation and transfers between paper spaces.

use std::{collections::HashSet, sync::Arc};

use super::{CanvasDocument, CanvasTool, ObjectEditing, ObjectGesture, Point, Rect, drag_rect};
use crate::{
    document::{InkTransform, Size, StrokeRecord},
    ink::StrokeTarget,
    render::{CanvasScene, SelectionOverlay, SelectionPreview},
};
use uuid::Uuid;

pub(super) enum SelectionGesture {
    Region { points: Vec<Point>, dragging: bool },
    Transform(Manipulation),
}

pub(super) struct Manipulation {
    ids: HashSet<Uuid>,
    strokes: Vec<Arc<StrokeRecord>>,
    frame: SelectionFrame,
    transform: InkTransform,
    rotation: bool,
    previous_angle: f64,
    angle: f64,
    dragging: bool,
}

#[derive(Clone, Copy)]
struct SelectionFrame {
    bounds: Rect,
    anchor: Point,
    handle: Point,
}

impl SelectionFrame {
    fn new(
        strokes: &[Arc<StrokeRecord>],
        ids: &HashSet<Uuid>,
        scale: f64,
        size: Size,
    ) -> Option<Self> {
        let mut bounds: Option<Rect> = None;
        for stroke in strokes.iter().filter(|s| ids.contains(&s.id)) {
            let b = stroke.bounds();
            bounds = Some(bounds.map_or(b, |a| {
                Rect::new(
                    a.min.x.min(b.min.x),
                    a.min.y.min(b.min.y),
                    a.max.x.max(b.max.x),
                    a.max.y.max(b.max.y),
                )
            }));
        }
        let bounds = bounds?;
        let padding = 5.0 / scale;
        let bounds = Rect::new(
            bounds.min.x - padding,
            bounds.min.y - padding,
            bounds.max.x + padding,
            bounds.max.y + padding,
        );
        let inset = (9.0 / scale).min(size.width * 0.5).min(size.height * 0.5);
        let above = bounds.min.y - 25.0 / scale >= inset;
        let anchor = Point::new(
            bounds.center().x,
            if above { bounds.min.y } else { bounds.max.y },
        );
        let handle = Point::new(
            anchor.x.clamp(inset, size.width - inset),
            (anchor.y + if above { -25.0 / scale } else { 25.0 / scale })
                .clamp(inset, size.height - inset),
        );
        Some(Self {
            bounds,
            anchor,
            handle,
        })
    }

    fn handle_hit(self, p: Point, scale: f64) -> bool {
        (p.x - self.handle.x).hypot(p.y - self.handle.y) <= 9.0 / scale
    }

    fn overlay(self, transform: InkTransform) -> SelectionOverlay {
        let a = self.bounds.min;
        let b = self.bounds.max;
        SelectionOverlay {
            corners: [a, Point::new(b.x, a.y), b, Point::new(a.x, b.y)].map(|p| transform.apply(p)),
            rotation_anchor: transform.apply(self.anchor),
            rotation_handle: transform.apply(self.handle),
        }
    }
}

impl Manipulation {
    fn transform_for(&self, source: StrokeTarget, destination: StrokeTarget) -> InkTransform {
        if source.page_index == destination.page_index {
            self.transform
        } else {
            self.transform.followed_by(source.transform_to(destination))
        }
    }

    fn update(&mut self, gesture: ObjectGesture) {
        let delta = Point::new(
            gesture.position.x - gesture.start.x,
            gesture.position.y - gesture.start.y,
        );
        self.dragging |= delta.x.hypot(delta.y) * gesture.target.view.scale() > 4.0;
        if self.rotation {
            let center = self.frame.bounds.center();
            let p = Point::new(gesture.position.x - center.x, gesture.position.y - center.y);
            // Crossing the pivot must not produce an undefined angle or a jump.
            if p.x.hypot(p.y) * gesture.target.view.scale() < 2.0 {
                return;
            }
            let angle = p.y.atan2(p.x);
            let delta = angle - self.previous_angle;
            self.angle += delta.sin().atan2(delta.cos());
            self.previous_angle = angle;
            if self.dragging {
                self.transform = InkTransform {
                    center,
                    radians: self.angle,
                    ..Default::default()
                };
            }
        } else if self.dragging {
            self.transform.translation = delta;
        }
    }
}

impl ObjectEditing {
    fn begin_selection(&mut self, gesture: ObjectGesture, document: &CanvasDocument) {
        self.selection_before = Some(self.selected.clone());
        let strokes = document.strokes(gesture.target.page_index);
        let scale = gesture.target.view.scale();
        if !gesture.additive
            && let Some(frame) = SelectionFrame::new(
                strokes,
                &self.selected,
                scale,
                gesture.target.original.paper_size(),
            )
        {
            let rotation = frame.handle_hit(gesture.start, scale);
            if rotation || frame.bounds.contains(gesture.start) {
                let center = frame.bounds.center();
                self.selection_gesture = Some(SelectionGesture::Transform(Manipulation {
                    ids: strokes
                        .iter()
                        .filter(|s| self.selected.contains(&s.id))
                        .map(|s| s.id)
                        .collect(),
                    strokes: strokes
                        .iter()
                        .filter(|s| self.selected.contains(&s.id))
                        .cloned()
                        .collect(),
                    frame,
                    rotation,
                    transform: InkTransform::default(),
                    previous_angle: (gesture.start.y - center.y).atan2(gesture.start.x - center.x),
                    angle: 0.0,
                    dragging: false,
                }));
                return;
            }
        }
        self.selection_gesture = Some(SelectionGesture::Region {
            points: vec![gesture.start],
            dragging: false,
        });
    }

    pub(super) fn update_selection(&mut self, gesture: ObjectGesture, document: &CanvasDocument) {
        if self.selection_gesture.is_none() {
            self.begin_selection(gesture, document);
        }
        match self.selection_gesture.as_mut().expect("selection gesture") {
            SelectionGesture::Transform(manipulation) => manipulation.update(gesture),
            SelectionGesture::Region { points, dragging } => {
                let scale = gesture.target.view.scale();
                *dragging |= (gesture.position.x - gesture.start.x)
                    .hypot(gesture.position.y - gesture.start.y)
                    * scale
                    > 4.0;
                if gesture.tool == CanvasTool::Lasso {
                    let last = *points.last().expect("lasso start");
                    if (gesture.position.x - last.x).hypot(gesture.position.y - last.y) * scale
                        >= 1.5
                    {
                        points.push(gesture.position);
                    }
                }
                self.selected = if gesture.additive {
                    self.selection_before.clone().unwrap_or_default()
                } else {
                    HashSet::new()
                };
                let strokes = document.strokes(gesture.target.page_index);
                if *dragging {
                    if gesture.tool == CanvasTool::Select {
                        let rect = drag_rect(gesture);
                        self.selected.extend(
                            strokes
                                .iter()
                                .filter(|s| {
                                    let b = s.bounds();
                                    rect.contains(b.min) && rect.contains(b.max)
                                })
                                .map(|s| s.id),
                        );
                    }
                } else if let Some(stroke) = strokes
                    .iter()
                    .rev()
                    .find(|s| s.hit_segment(gesture.position, gesture.position, 5.0 / scale))
                    && !self.selected.insert(stroke.id)
                {
                    self.selected.remove(&stroke.id);
                }
            }
        }
    }

    pub(super) fn finish_selection(&mut self, document: &mut CanvasDocument) {
        match self.selection_gesture.take() {
            Some(SelectionGesture::Transform(manipulation)) => {
                if manipulation.rotation {
                    document.transform_strokes(&manipulation.ids, manipulation.transform);
                } else if manipulation.dragging
                    && let Some(gesture) = self.gesture
                    && let Some(destination) = gesture.destination
                {
                    document.move_strokes(
                        &manipulation.ids,
                        manipulation.transform_for(gesture.target, destination),
                        destination.page_index,
                    );
                }
            }
            Some(SelectionGesture::Region {
                mut points,
                dragging: true,
            }) => {
                if let Some(gesture) = self.gesture.filter(|g| g.tool == CanvasTool::Lasso) {
                    if points.last() != Some(&gesture.position) {
                        points.push(gesture.position);
                    }
                    self.selected.extend(
                        document
                            .strokes(gesture.target.page_index)
                            .iter()
                            .filter(|s| s.enclosed_by_lasso(&points))
                            .map(|s| s.id),
                    );
                }
            }
            _ => {}
        }
    }

    pub fn decorate(&self, scene: &mut CanvasScene) {
        let gesture = self.gesture.filter(|g| g.tool.is_selection());
        for page in &mut scene.pages {
            page.strokes.retain(|s| !self.erased.contains(&s.id));
            page.selected_ids = page
                .strokes
                .iter()
                .filter(|s| self.selected.contains(&s.id))
                .map(|s| s.id)
                .collect();
            if let Some(gesture) = gesture {
                match &self.selection_gesture {
                    Some(SelectionGesture::Transform(manipulation)) => {
                        let target = StrokeTarget {
                            page_index: page.index,
                            original: page.source_page,
                            displayed: page.page,
                            view: page.view,
                            viewport: scene.viewport,
                        };
                        let transform = manipulation.transform_for(gesture.target, target);
                        if page.index == gesture.target.page_index {
                            page.selection_transform = Some(manipulation.transform);
                        } else if !manipulation.rotation && manipulation.dragging {
                            page.selection_preview = Some(SelectionPreview {
                                strokes: manipulation.strokes.clone(),
                                transform,
                            });
                        }
                        let overlay_page = if manipulation.rotation {
                            gesture.target.page_index
                        } else {
                            gesture
                                .destination
                                .map_or(gesture.target.page_index, |t| t.page_index)
                        };
                        if page.index == overlay_page {
                            page.selection_overlay = Some(manipulation.frame.overlay(transform));
                            continue;
                        }
                        if page.index == gesture.target.page_index {
                            continue;
                        }
                    }
                    Some(SelectionGesture::Region {
                        points,
                        dragging: true,
                    }) if gesture.target.page_index == page.index => {
                        if gesture.tool == CanvasTool::Lasso {
                            page.selection_path = points.clone();
                            if page.selection_path.last() != Some(&gesture.position) {
                                page.selection_path.push(gesture.position);
                            }
                        } else {
                            page.selection_rect = Some(drag_rect(gesture));
                        }
                    }
                    _ => {}
                }
            }
            page.selection_overlay = SelectionFrame::new(
                &page.strokes,
                &self.selected,
                page.view.scale(),
                page.source_page.paper_size(),
            )
            .map(|frame| frame.overlay(InkTransform::default()));
        }
    }

    pub fn cursor(
        &self,
        document: &CanvasDocument,
        view: &super::super::view::CanvasView,
        physical: Point,
    ) -> Option<egui::CursorIcon> {
        if matches!(self.selection_gesture, Some(SelectionGesture::Transform(_))) {
            if let Some(SelectionGesture::Transform(manipulation)) = &self.selection_gesture
                && !manipulation.rotation
                && manipulation.dragging
                && self.gesture.is_some_and(|g| g.destination.is_none())
            {
                return Some(egui::CursorIcon::NotAllowed);
            }
            return Some(egui::CursorIcon::Grabbing);
        }
        let target = view.stroke_target(physical)?;
        let frame = SelectionFrame::new(
            document.strokes(target.page_index),
            &self.selected,
            target.view.scale(),
            target.original.paper_size(),
        )?;
        let paper = target.paper_position(physical);
        if frame.handle_hit(paper, target.view.scale()) {
            Some(egui::CursorIcon::Grab)
        } else if frame.bounds.contains(paper) {
            Some(egui::CursorIcon::Move)
        } else {
            None
        }
    }
}
