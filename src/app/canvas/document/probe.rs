//! Simulated pressure-stroke overlay and separate test-PDF export controls.

use std::sync::Arc;

use super::CanvasDocument;

impl CanvasDocument {
    pub fn can_demo(&self) -> bool {
        self.loaded.is_some() && self.pending.is_none() && self.pending_export.is_none()
    }

    pub fn can_export_demo(&self) -> bool {
        self.can_demo()
            && self.strokes.is_empty()
            && self
                .loaded
                .as_ref()
                .is_some_and(|loaded| loaded.demo_stroke.is_some())
    }

    pub fn toggle_demo(&mut self) {
        if !self.can_demo() {
            return;
        }
        let loaded = self.loaded.as_mut().expect("loaded document");
        if loaded.demo_stroke.take().is_none() {
            match crate::ink::demo_stroke(loaded.pdf.first_page.geometry) {
                Ok(stroke) => {
                    loaded.demo_stroke = Some(Arc::new(stroke));
                    self.message = Some(
                        "Simulated pressure stroke | Export test PDF to save a separate copy"
                            .to_owned(),
                    );
                }
                Err(error) => self.set_error(format!("模拟笔迹失败：{error:#}")),
            }
        } else {
            self.message = Some("Demo overlay removed".to_owned());
        }
    }

    pub fn export_demo(&mut self) {
        if !self.can_export_demo() {
            return;
        }
        self.notice = None;
        let stroke = Arc::clone(self.loaded.as_ref().unwrap().demo_stroke.as_ref().unwrap());
        let document = self.loaded_request.unwrap();
        match self.worker.export_probe(document, stroke) {
            Ok(request) => {
                self.pending_export = Some((document, request));
                self.message = Some(
                    "Saving and verifying a test copy in .scratch/pdf-save-probe/...".to_owned(),
                );
            }
            Err(error) => self.set_error(format!("测试导出失败：{error:#}")),
        }
    }
}
