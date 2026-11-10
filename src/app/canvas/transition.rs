//! Pending destructive intent; cancellation never changes the document/history.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DocumentAction {
    Open(PathBuf),
    Close,
    Exit,
}

#[derive(Default)]
pub(super) struct DocumentTransition {
    pending: Option<DocumentAction>,
}

impl DocumentTransition {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn request(&mut self, action: DocumentAction, dirty: bool) -> Option<DocumentAction> {
        // Repeated close events or dropped files cannot change an existing prompt.
        if self.is_pending() {
            return None;
        }
        if dirty {
            self.pending = Some(action);
            None
        } else {
            Some(action)
        }
    }

    pub fn resolve(&mut self, discard: bool) -> Option<DocumentAction> {
        self.pending.take().filter(|_| discard)
    }

    pub fn show(&self, context: &egui::Context, title: &str) -> Option<bool> {
        let action = self.pending.as_ref()?;
        let verb = match action {
            DocumentAction::Open(_) => "打开其他文档",
            DocumentAction::Close => "关闭文档",
            DocumentAction::Exit => "退出应用",
        };
        let modal = egui::Modal::new(egui::Id::unique("unsaved_changes"))
            .frame(crate::app::theme::popup_frame())
            .show(context, |ui| {
                ui.set_width(360.0_f32.min((context.content_rect().width() - 48.0).max(200.0)));
                ui.heading("未保存的更改");
                ui.add_space(8.0);
                ui.label(format!("「{title}」有未保存的笔迹。{verb}将丢弃这些更改。"));
                ui.label("当前版本尚不支持保存，可取消并返回文档。");
                ui.add_space(12.0);
                let mut answer = None;
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        answer = Some(false);
                    }
                    ui.add_enabled(false, egui::Button::new("保存"))
                        .on_disabled_hover_text("保存功能尚未接入");
                    if ui.button("丢弃更改").clicked() {
                        answer = Some(true);
                    }
                });
                answer
            });
        modal
            .inner
            .or_else(|| modal.should_close().then_some(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_transitions_are_immediate() {
        for action in [
            DocumentAction::Open("a.pdf".into()),
            DocumentAction::Close,
            DocumentAction::Exit,
        ] {
            let mut transition = DocumentTransition::default();
            assert_eq!(transition.request(action.clone(), false), Some(action));
            assert!(!transition.is_pending());
        }
    }

    #[test]
    fn cancelling_each_dirty_transition_keeps_the_current_document() {
        for action in [
            DocumentAction::Open("a.pdf".into()),
            DocumentAction::Close,
            DocumentAction::Exit,
        ] {
            let mut transition = DocumentTransition::default();
            assert!(transition.request(action, true).is_none());
            assert!(transition.is_pending());
            assert!(transition.resolve(false).is_none());
            assert!(!transition.is_pending());
            assert!(transition.resolve(true).is_none());
        }
    }

    #[test]
    fn discard_executes_exactly_the_original_intent_once() {
        let mut transition = DocumentTransition::default();
        let path = PathBuf::from("中文 PDF.pdf");
        assert!(
            transition
                .request(DocumentAction::Open(path.clone()), true)
                .is_none()
        );
        assert!(transition.request(DocumentAction::Exit, true).is_none());
        assert_eq!(transition.resolve(true), Some(DocumentAction::Open(path)));
        assert!(transition.resolve(true).is_none());
        assert!(!transition.is_pending());
    }
}
