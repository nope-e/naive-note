//! Pending destructive intent; cancellation never changes the document/history.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DocumentAction {
    Open(PathBuf),
    Recover(PathBuf),
    Close,
    Exit,
}

#[derive(Default)]
pub(super) struct DocumentTransition {
    pending: Option<DocumentAction>,
    waiting_for_save: bool,
}

#[derive(Clone, Copy)]
pub(super) enum TransitionDecision {
    Cancel,
    Save,
    SaveAs,
    Discard,
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
        self.waiting_for_save = false;
        self.pending.take().filter(|_| discard)
    }

    pub fn wait_for_save(&mut self) {
        self.waiting_for_save = true;
    }

    pub fn save_finished(&mut self, clean_success: bool) -> Option<DocumentAction> {
        if !std::mem::take(&mut self.waiting_for_save) {
            return None;
        }
        if clean_success {
            self.resolve(true)
        } else {
            None
        }
    }

    pub fn show(
        &self,
        context: &egui::Context,
        title: &str,
        busy: bool,
        can_save: bool,
        notice: Option<&str>,
    ) -> Option<TransitionDecision> {
        let action = self.pending.as_ref()?;
        let verb = match action {
            DocumentAction::Open(_) | DocumentAction::Recover(_) => "打开其他文档",
            DocumentAction::Close => "关闭文档",
            DocumentAction::Exit => "退出应用",
        };
        let modal = egui::Modal::new(egui::Id::unique("unsaved_changes"))
            .frame(crate::app::theme::popup_frame())
            .show(context, |ui| {
                ui.set_width(360.0_f32.min((context.content_rect().width() - 48.0).max(200.0)));
                ui.heading("未保存的更改");
                ui.add_space(8.0);
                ui.label(format!("是否保存「{title}」的更改后{verb}？"));
                if busy {
                    ui.label("正在保存 PDF，请稍候…");
                }
                if let Some(notice) = notice {
                    ui.colored_label(egui::Color32::LIGHT_RED, notice);
                }
                ui.add_space(12.0);
                let mut answer = None;
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        answer = Some(TransitionDecision::Cancel);
                    }
                    if ui
                        .add_enabled(can_save, egui::Button::new("保存"))
                        .clicked()
                    {
                        answer = Some(TransitionDecision::Save);
                    }
                    if ui
                        .add_enabled(can_save, egui::Button::new("另存为…"))
                        .clicked()
                    {
                        answer = Some(TransitionDecision::SaveAs);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("丢弃更改"))
                        .clicked()
                    {
                        answer = Some(TransitionDecision::Discard);
                    }
                });
                answer
            });
        modal
            .inner
            .or_else(|| modal.should_close().then_some(TransitionDecision::Cancel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_keeps_the_original_intent_until_success_and_cancel_clears_it() {
        let mut transition = DocumentTransition::default();
        transition.request(DocumentAction::Exit, true);
        transition.wait_for_save();
        assert!(transition.save_finished(false).is_none());
        assert!(transition.is_pending());
        transition.wait_for_save();
        assert_eq!(transition.save_finished(true), Some(DocumentAction::Exit));
        assert!(!transition.is_pending());
        assert!(transition.save_finished(true).is_none());
        transition.request(DocumentAction::Close, true);
        transition.wait_for_save();
        transition.resolve(false);
        assert!(transition.save_finished(true).is_none());
    }

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
