//! Wall-clock autosave and the startup recovery chooser, independent of redraws.

use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use anyhow::Result;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::UserEvent,
    storage::preferences::{self, DraftCopy, Settings, SettingsWorker},
};

pub(super) struct AutosaveTimer {
    next: Instant,
    interval: Duration,
}

impl AutosaveTimer {
    pub fn new(now: Instant, seconds: u64) -> Self {
        let interval = Duration::from_secs(seconds.clamp(5, 15));
        Self {
            next: now + interval,
            interval,
        }
    }
    pub fn deadline(&self) -> Instant {
        self.next
    }
    pub fn take_due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        // A slow save/suspend never generates a backlog of missed timer ticks.
        self.next = now + self.interval;
        true
    }
}

pub(super) enum RecoveryAction {
    Restore(PathBuf),
    Delete(PathBuf),
}

pub(super) struct WorkspacePersistence {
    pub worker: Option<SettingsWorker>,
    pub root: Option<PathBuf>,
    pub settings: Settings,
    pub settings_changed: bool,
    pub timer: AutosaveTimer,
    pub drafts: Vec<DraftCopy>,
    pub recovery_open: bool,
    pub deleting: Vec<PathBuf>,
}

impl Default for WorkspacePersistence {
    fn default() -> Self {
        Self {
            worker: None,
            root: None,
            settings: Settings::default(),
            settings_changed: false,
            timer: AutosaveTimer::new(Instant::now(), 10),
            drafts: Vec::new(),
            recovery_open: false,
            deleting: Vec::new(),
        }
    }
}

impl WorkspacePersistence {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let root = preferences::app_data_dir()?;
        let worker = SettingsWorker::new(root.clone(), move |response| {
            let _ = proxy.send_event(UserEvent::StorageResponse(Box::new(response)));
        })?;
        Ok(Self {
            worker: Some(worker),
            root: Some(root),
            ..Self::default()
        })
    }

    pub fn show_recovery(&mut self, context: &egui::Context) -> Option<RecoveryAction> {
        if !self.recovery_open {
            return None;
        }
        let modal = egui::Modal::new(egui::Id::unique("recover_drafts"))
            .frame(crate::app::theme::popup_frame())
            .show(context, |ui| {
                ui.set_width(440.0_f32.min((context.content_rect().width() - 48.0).max(200.0)));
                ui.heading("恢复未命名草稿");
                ui.label("副本保留最近一次成功自动保存的内容。恢复后仍需 Ctrl+S 选择正式路径。");
                ui.add_space(8.0);
                let mut action = None;
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for draft in &self.drafts {
                            ui.group(|ui| {
                                let age = draft
                                    .modified
                                    .and_then(|time| SystemTime::now().duration_since(time).ok())
                                    .map_or_else(
                                        || "时间未知".to_owned(),
                                        |age| {
                                            if age.as_secs() < 60 {
                                                "刚刚".to_owned()
                                            } else if age.as_secs() < 3600 {
                                                format!("{} 分钟前", age.as_secs() / 60)
                                            } else {
                                                format!("{} 小时前", age.as_secs() / 3600)
                                            }
                                        },
                                    );
                                ui.label(format!(
                                    "草稿 {} · {:.1} KiB · {age}",
                                    &draft.id.to_string()[..8],
                                    draft.bytes as f64 / 1024.0
                                ));
                                ui.horizontal(|ui| {
                                    let enabled = !self.deleting.contains(&draft.path);
                                    if ui.add_enabled(enabled, egui::Button::new("恢复")).clicked()
                                    {
                                        action = Some(RecoveryAction::Restore(draft.path.clone()));
                                    }
                                    if ui
                                        .add_enabled(enabled, egui::Button::new("删除副本"))
                                        .clicked()
                                    {
                                        action = Some(RecoveryAction::Delete(draft.path.clone()));
                                    }
                                });
                            });
                        }
                        if self.drafts.is_empty() {
                            ui.label("没有待恢复的草稿副本。");
                        }
                    });
                ui.add_space(8.0);
                if ui.button("稍后处理").clicked() {
                    self.recovery_open = false;
                }
                action
            });
        if modal.should_close() {
            self.recovery_open = false;
        }
        modal.inner
    }
}

impl super::CanvasWorkspace {
    pub fn storage_response(&mut self, response: crate::storage::preferences::StorageResponse) {
        use crate::storage::preferences::StorageResponse;
        match response {
            StorageResponse::Loaded {
                settings,
                drafts,
                warnings,
            } => {
                if !self.persistence.settings_changed {
                    self.input.style.color_rgb = settings.color;
                    self.input.style.max_width_pt = settings.width;
                    self.input.style.pressure_gamma = settings.pressure_gamma;
                    self.input.highlighter_style.color_rgb = settings.highlighter_color;
                    self.input.highlighter_style.max_width_pt = settings.highlighter_width;
                    self.input.highlighter_style.pressure_gamma =
                        settings.highlighter_pressure_gamma;
                    self.input.smoothing = settings.smoothing;
                    self.input.mouse_pen = settings.mouse_pen;
                    self.persistence.timer =
                        AutosaveTimer::new(Instant::now(), settings.autosave_seconds);
                    self.persistence.settings = settings;
                    self.document
                        .set_global_reference(self.persistence.settings.reference_lines);
                }
                self.persistence.recovery_open = !drafts.is_empty();
                if self.persistence.recovery_open {
                    self.input.cancel_drawing();
                }
                self.persistence.drafts = drafts;
                if !warnings.is_empty() {
                    self.document.set_error(warnings.join("；"));
                }
            }
            StorageResponse::SettingsSaved(Err(error)) => self.document.set_error(error),
            StorageResponse::SettingsSaved(Ok(())) => {}
        }
    }

    pub fn autosave_deadline(&self) -> Instant {
        self.persistence.timer.deadline()
    }

    pub fn maintenance(&mut self, now: Instant) -> bool {
        if !self.persistence.timer.take_due(now) {
            return false;
        }
        if self.transition.is_pending() || self.save_requested.is_some() {
            return false;
        }
        self.document.autosave(self.view.draft_page())
    }

    pub(super) fn persist_settings(&mut self) {
        let settings = crate::storage::preferences::Settings {
            color: self.input.style.color_rgb,
            width: self.input.style.max_width_pt,
            pressure_gamma: self.input.style.pressure_gamma,
            highlighter_color: self.input.highlighter_style.color_rgb,
            highlighter_width: self.input.highlighter_style.max_width_pt,
            highlighter_pressure_gamma: self.input.highlighter_style.pressure_gamma,
            smoothing: self.input.smoothing,
            mouse_pen: self.input.mouse_pen,
            ..self.persistence.settings.clone()
        };
        self.persistence.settings_changed = true;
        self.persistence.settings = settings.clone();
        if let Some(worker) = &self.persistence.worker {
            worker.save(settings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_keeps_a_deadline_during_continuous_edits_and_skips_missed_ticks() {
        let start = Instant::now();
        let mut timer = AutosaveTimer::new(start, 10);
        for seconds in 0..10 {
            assert!(!timer.take_due(start + Duration::from_secs(seconds)));
        }
        assert!(timer.take_due(start + Duration::from_secs(10)));
        assert_eq!(timer.deadline(), start + Duration::from_secs(20));
        assert!(timer.take_due(start + Duration::from_secs(70)));
        assert_eq!(timer.deadline(), start + Duration::from_secs(80));
        assert!(!timer.take_due(start + Duration::from_secs(70)));
    }
}
