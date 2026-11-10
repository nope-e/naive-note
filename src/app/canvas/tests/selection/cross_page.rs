//! Cross-page input, preview geometry and history only; no windows or pixels.

use super::*;
use crate::{
    document::{InkKind, PenStyle, StrokeRecord},
    ink::LiveStroke,
};

fn target(workspace: &CanvasWorkspace, index: u32) -> StrokeTarget {
    let viewport = workspace.view.viewport.unwrap();
    let page = workspace
        .view
        .scenes(viewport, true)
        .into_iter()
        .find(|s| s.index == index)
        .unwrap();
    StrokeTarget {
        page_index: index,
        original: page.source_page,
        displayed: page.page,
        view: page.view,
        viewport,
    }
}

fn center(target: StrokeTarget) -> Point {
    let size = target.original.paper_size();
    Point::new(size.width * 0.5, size.height * 0.5)
}

fn record(target: StrokeTarget, y_offset: f64, kind: InkKind) -> Arc<StrokeRecord> {
    let center = center(target);
    let mut live = LiveStroke::new(
        target,
        false,
        PenStyle {
            kind,
            ..Default::default()
        },
    );
    live.push_mouse(
        target.physical_position(Point::new(center.x - 15., center.y + y_offset)),
        0,
        crate::document::PenPhase::Down,
    );
    live.push_mouse(
        target.physical_position(Point::new(center.x + 15., center.y + y_offset)),
        1,
        crate::document::PenPhase::Up,
    );
    live.finish().unwrap()
}

fn setup(rotation: Rotation, turns: u32, dpi: f64) -> CanvasWorkspace {
    let mut workspace = workspace();
    // Synthetic mixed geometry layout; real PDF page reassignment is checked by
    // structural saving tests. These input events do not invoke page expansion.
    let pages = vec![
        PageGeometry::new(Rect::new(20., 30., 620., 830.), rotation, 1.5).unwrap(),
        PageGeometry::new(Rect::new(-40., 10., 360., 310.), rotation.next(), 1.2).unwrap(),
    ];
    workspace.view = CanvasView::new(pages);
    let viewport = CanvasViewport::new(
        Rect::new(0., 0., 1200., 1600.),
        dpi,
        ((1200. * dpi) as u32, (1600. * dpi) as u32),
    )
    .unwrap();
    workspace.view.update_viewport(viewport);
    for _ in 0..turns {
        workspace
            .view
            .apply_action(ViewAction::Rotate, viewport.logical);
    }
    workspace
        .view
        .apply_action(ViewAction::Zoom(0.35), viewport.logical);
    workspace.input.tool = if turns.is_multiple_of(2) {
        CanvasTool::Select
    } else {
        CanvasTool::Lasso
    };
    workspace.input.mouse_pen = false;
    workspace
}

fn scene(workspace: &CanvasWorkspace) -> CanvasScene {
    let viewport = workspace.view.viewport.unwrap();
    let mut scene = CanvasScene {
        viewport,
        pages: workspace.view.scenes(viewport, true),
    };
    for page in &mut scene.pages {
        page.strokes = workspace.document.strokes(page.index).to_vec();
    }
    workspace.objects.decorate(&mut scene);
    scene
}

#[test]
fn both_devices_transfer_a_group_in_both_directions_without_a_preview_or_commit_jump() {
    for mouse in [false, true] {
        for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            for turns in 0..4 {
                for dpi in [1., 1.5, 2.] {
                    for reverse in [false, true] {
                        let mut workspace = setup(rotation, turns, dpi);
                        let source = target(&workspace, u32::from(reverse));
                        let destination = target(&workspace, u32::from(!reverse));
                        let a = record(source, -10., InkKind::Pen);
                        let b = record(source, 10., InkKind::Highlighter);
                        for s in [&a, &b] {
                            workspace.document.add_stroke(s.clone());
                        }
                        workspace
                            .objects
                            .select_page(&workspace.document, source.page_index);
                        let untouched = record(source, 80., InkKind::Pen);
                        let existing = record(destination, 0., InkKind::Pen);
                        workspace.document.add_stroke(untouched.clone());
                        workspace.document.add_stroke(existing.clone());
                        let revision = workspace.document.revision();
                        let before_view = workspace.view.transform;
                        let start = source.physical_position(center(source));
                        let end = destination.physical_position(center(destination));
                        assert_eq!(
                            workspace.view.stroke_target(start).unwrap().page_index,
                            source.page_index
                        );
                        assert_eq!(
                            workspace.view.stroke_target(end).unwrap().page_index,
                            destination.page_index
                        );
                        contact(&mut workspace, mouse, PenPhase::Down, start);
                        contact(&mut workspace, mouse, PenPhase::Move, end);
                        assert_eq!(workspace.document.revision(), revision);
                        assert_eq!(workspace.document.strokes(source.page_index).len(), 3);
                        assert_eq!(workspace.document.strokes(destination.page_index).len(), 1);
                        let preview_scene = scene(&workspace);
                        let page = preview_scene
                            .pages
                            .iter()
                            .find(|p| p.index == destination.page_index)
                            .unwrap();
                        let preview = page.selection_preview.as_ref().unwrap();
                        assert!(page.selection_overlay.is_some());
                        assert_eq!(
                            preview.strokes.iter().map(|s| s.id).collect::<Vec<_>>(),
                            vec![a.id, b.id]
                        );
                        for s in [&a, &b] {
                            let old = source.physical_position(s.modeled_samples[0].position);
                            let expected =
                                Point::new(old.x + end.x - start.x, old.y + end.y - start.y);
                            close(
                                destination.physical_position(
                                    preview.transform.apply(s.modeled_samples[0].position),
                                ),
                                expected,
                            );
                        }
                        contact(&mut workspace, mouse, PenPhase::Up, end);
                        assert_eq!(workspace.document.revision().current, revision.current + 1);
                        assert_eq!(workspace.view.transform, before_view);
                        assert_eq!(workspace.document.strokes(source.page_index).len(), 1);
                        assert!(Arc::ptr_eq(
                            &workspace.document.strokes(source.page_index)[0],
                            &untouched
                        ));
                        let moved = workspace.document.strokes(destination.page_index).to_vec();
                        assert!(Arc::ptr_eq(&moved[0], &existing));
                        assert_eq!(workspace.objects.selected_count(), 2);
                        for (s, original) in moved[1..].iter().zip([&a, &b]) {
                            assert_eq!(s.id, original.id);
                            assert_eq!(s.page_index, destination.page_index);
                            assert_eq!(s.raw_samples, original.raw_samples);
                            assert_eq!(s.style, original.style);
                            close(
                                s.modeled_samples[0].position,
                                preview
                                    .transform
                                    .apply(original.modeled_samples[0].position),
                            );
                        }
                        assert!(
                            scene(&workspace)
                                .pages
                                .iter()
                                .all(|p| p.selection_preview.is_none())
                        );
                        workspace.document.undo();
                        for (s, original) in workspace
                            .document
                            .strokes(source.page_index)
                            .iter()
                            .zip([&a, &b, &untouched])
                        {
                            assert!(Arc::ptr_eq(s, original));
                        }
                        assert_eq!(workspace.document.strokes(destination.page_index).len(), 1);
                        assert_eq!(workspace.document.revision(), revision);
                        workspace.document.redo();
                        for (s, original) in workspace
                            .document
                            .strokes(destination.page_index)
                            .iter()
                            .zip(&moved)
                        {
                            assert!(Arc::ptr_eq(s, original));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn outside_release_cancel_save_and_navigation_never_commit_a_pending_transfer() {
    for mouse in [false, true] {
        for cancel_mode in 0..4 {
            let mut workspace = setup(Rotation::R90, 1, 1.5);
            let source = target(&workspace, 0);
            let destination = target(&workspace, 1);
            let original = record(source, 0., InkKind::Pen);
            workspace.document.add_stroke(original.clone());
            workspace.objects.select_page(&workspace.document, 0);
            let revision = workspace.document.revision();
            let start = source.physical_position(center(source));
            let mut end = destination.physical_position(center(destination));
            contact(&mut workspace, mouse, PenPhase::Down, start);
            contact(&mut workspace, mouse, PenPhase::Move, end);
            match cancel_mode {
                0 => {
                    end = source.viewport.logical_to_physical(Point::new(1., 1.));
                    assert!(workspace.view.stroke_target(end).is_none());
                    contact(&mut workspace, mouse, PenPhase::Move, end);
                    assert_eq!(
                        workspace
                            .objects
                            .cursor(&workspace.document, &workspace.view, end),
                        Some(egui::CursorIcon::NotAllowed)
                    );
                }
                1 => workspace.cancel_input(),
                2 => workspace.request_save(false),
                _ => workspace.modifiers_changed(egui::Modifiers::CTRL),
            }
            if mouse {
                workspace.mouse_button(false, Some(end), egui::Modifiers::NONE, false);
            } else {
                contact(&mut workspace, mouse, PenPhase::Up, end);
            }
            assert_eq!(workspace.document.revision(), revision);
            assert!(Arc::ptr_eq(&workspace.document.strokes(0)[0], &original));
            assert!(workspace.document.strokes(1).is_empty());
            assert_eq!(workspace.objects.selected_count(), 1);
            assert!(!workspace.input.is_drawing());
            assert!(
                scene(&workspace)
                    .pages
                    .iter()
                    .all(|p| p.selection_preview.is_none())
            );
        }
    }
}
