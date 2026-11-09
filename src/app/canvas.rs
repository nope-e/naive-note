//! Compose workspace widgets and route document/view actions to their owners.

mod area;
mod document;
mod ink_input;
mod pdf_preview;
mod scrollbars;
mod status_bar;
mod toolbar;
mod view;

use std::path::PathBuf;

use anyhow::Result;
use area::CanvasArea;
use document::CanvasDocument;
use ink_input::CanvasInput;
use toolbar::ToolbarAction;
use view::CanvasView;
use winit::event_loop::EventLoopProxy;

use crate::{document::Point, pdf::PdfResponse, render::CanvasScene};

use super::UserEvent;

pub(super) struct CanvasWorkspace {
    view: CanvasView,
    area: CanvasArea,
    document: CanvasDocument,
    input: CanvasInput,
    open_requested: bool,
}

impl CanvasWorkspace {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        Ok(Self {
            view: CanvasView::default(),
            area: CanvasArea::default(),
            document: CanvasDocument::new(proxy)?,
            input: CanvasInput::default(),
            open_requested: false,
        })
    }

    pub fn open_pdf(&mut self, path: PathBuf) {
        self.input.cancel_drawing();
        self.document.open(path);
    }

    pub fn pdf_response(&mut self, response: PdfResponse) {
        if let Some(page) = self.document.receive(response) {
            self.input.cancel_drawing();
            self.view = CanvasView::new(page);
            self.area = CanvasArea::default();
        }
    }

    pub fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }

    fn close_pdf(&mut self) {
        self.input.cancel_drawing();
        self.document.close();
        self.view = CanvasView::default();
        self.area = CanvasArea::default();
    }

    pub fn cancel_input(&mut self) {
        self.input.cancel();
    }

    pub fn pen_sample(&mut self, event: crate::platform::PenEvent) -> bool {
        let (ui_owned, stroke) = self.input.pen(
            event.sample,
            &mut self.view,
            self.document.can_ink(),
            event.modifiers,
        );
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        ui_owned
    }

    pub fn mouse_button(
        &mut self,
        down: bool,
        position: Option<Point>,
        modifiers: egui::Modifiers,
    ) -> bool {
        let (owned, stroke) = self.input.mouse_button(
            down,
            position,
            &mut self.view,
            self.document.can_ink(),
            modifiers,
        );
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        owned
    }

    pub fn mouse_move(&mut self, position: Point, modifiers: egui::Modifiers) {
        if let Some(stroke) = self.input.mouse_move(position, &mut self.view, modifiers) {
            self.document.add_stroke(stroke);
        }
    }

    pub fn modifiers_changed(&mut self, modifiers: egui::Modifiers) {
        if let Some(stroke) = self.input.modifiers_changed(modifiers, &mut self.view) {
            self.document.add_stroke(stroke);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        show_renderer_info: bool,
        fps: Option<f64>,
        framebuffer: (u32, u32),
        process_input: bool,
    ) -> Option<CanvasScene> {
        let before = (
            self.view.transform,
            self.view.current_page(),
            self.view.page(),
        );
        let mut action = toolbar::show(
            ui,
            self.view.transform.zoom(),
            self.view.page(),
            self.view.viewport,
            show_renderer_info,
            fps,
            toolbar::DocumentActions {
                can_close: self.document.can_close(),
                can_demo: self.document.can_demo(),
                can_export_demo: self.document.can_export_demo(),
                current_page: self.view.current_page(),
                page_count: self.view.page_count(),
                mouse_pen: self.input.mouse_pen,
                smoothing: self.input.smoothing,
            },
        );
        if process_input {
            if action.is_some() && !matches!(action, Some(ToolbarAction::Smoothing)) {
                self.input.cancel_drawing();
            }
            match action {
                Some(ToolbarAction::Open) => {
                    self.open_requested = true;
                    action = None;
                }
                Some(ToolbarAction::Close) => {
                    self.close_pdf();
                    action = None;
                }
                Some(ToolbarAction::DemoStroke) => {
                    self.document.toggle_demo();
                    action = None;
                }
                Some(ToolbarAction::ExportDemo) => {
                    self.document.export_demo();
                    action = None;
                }
                Some(ToolbarAction::MousePen) => {
                    self.input.mouse_pen = !self.input.mouse_pen;
                    action = None;
                }
                Some(ToolbarAction::Smoothing) => {
                    self.input.smoothing = !self.input.smoothing;
                    action = None;
                }
                _ => {}
            }
        }
        let document_label = self.document.label(self.view.current_page());
        let pointer = ui
            .input(|input| input.pointer.hover_pos())
            .and_then(|pointer| {
                self.view
                    .pointer_coordinates(Point::new(f64::from(pointer.x), f64::from(pointer.y)))
            });
        status_bar::show(
            ui,
            self.view.page(),
            self.view.transform.zoom(),
            self.view.fit_to_page,
            pointer,
            &document_label,
            self.document.message(),
        );

        let view_action = match action {
            Some(ToolbarAction::View(action)) => Some(action),
            _ => None,
        };
        let scene = self
            .area
            .show(
                ui,
                framebuffer,
                process_input && !self.input.is_drawing(),
                &mut self.view,
                view_action,
            )
            .map(|viewport| {
                let mut scene = CanvasScene {
                    viewport,
                    pages: self.view.scenes(viewport, !self.document.is_loaded()),
                };
                self.document
                    .update_scene(ui.ctx(), &mut scene, self.view.current_page());
                if let Some(stroke) = self.input.active()
                    && let Some(page) = scene
                        .pages
                        .iter_mut()
                        .find(|page| page.index == stroke.record.page_index)
                {
                    page.active_stroke = Some(stroke);
                }
                scene
            });

        // Refresh toolbar/status readouts after canvas input changes the view.
        if before
            != (
                self.view.transform,
                self.view.current_page(),
                self.view.page(),
            )
        {
            ui.ctx().request_repaint();
        }
        scene
    }
}
