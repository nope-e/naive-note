//! Shared popup typography and full-width rows.

use super::icons::{self, Icon};
use crate::app::theme;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, vec2};

pub(super) fn heading(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(title)
                .size(18.0)
                .strong()
                .color(theme::TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Close, "关闭", false, true).clicked() {
                ui.close();
            }
        });
    });
    ui.add_space(8.0);
}

pub(super) fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(label)
            .size(12.0)
            .strong()
            .color(theme::MUTED),
    );
    ui.add_space(4.0);
}

pub(super) fn menu_row(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 33.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 7.0, theme::HOVER);
        }
        let color = if enabled {
            theme::TEXT
        } else {
            Color32::from_rgb(119, 119, 130)
        };
        icons::paint(
            ui,
            Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), vec2(16.0, 16.0)),
            icon,
            color,
        );
        ui.painter().text(
            rect.left_center() + vec2(34.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.5),
            color,
        );
        ui.painter().text(
            rect.right_center() - vec2(9.0, 0.0),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(11.0),
            theme::MUTED.gamma_multiply(if enabled { 1.0 } else { 0.55 }),
        );
        response
    })
    .inner
    .on_disabled_hover_text("暂未接入")
}

pub(super) fn choice(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 61.0), Sense::click());
        let fill = if selected {
            theme::SELECTED
        } else if response.hovered() {
            theme::HOVER
        } else {
            theme::CARD
        };
        ui.painter().rect_filled(rect, 8.0, fill);
        if selected {
            ui.painter().rect_stroke(
                rect,
                8.0,
                Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.6)),
                StrokeKind::Inside,
            );
        }
        let color = if enabled {
            theme::TEXT
        } else {
            theme::MUTED.gamma_multiply(0.65)
        };
        ui.painter().text(
            rect.min + vec2(12.0, 10.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(13.0),
            color,
        );
        let text = ui.painter().layout(
            description.to_owned(),
            FontId::proportional(11.0),
            if enabled { theme::MUTED } else { color },
            rect.width() - 32.0,
        );
        ui.painter()
            .galley(rect.min + vec2(12.0, 31.0), text, color);
        if selected {
            icons::paint(
                ui,
                Rect::from_min_size(rect.right_top() + vec2(-29.0, 9.0), vec2(15.0, 15.0)),
                Icon::Check,
                theme::ACCENT,
            );
        }
        response
    })
    .inner
    .on_disabled_hover_text("曲线路径暂未接入")
}

pub(super) fn switch(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    value: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        9.0,
        if response.hovered() {
            theme::HOVER
        } else {
            theme::CARD
        },
    );
    ui.painter().text(
        rect.min + vec2(12.0, 9.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(13.0),
        theme::TEXT,
    );
    ui.painter().text(
        rect.min + vec2(12.0, 32.0),
        Align2::LEFT_TOP,
        description,
        FontId::proportional(11.0),
        theme::MUTED,
    );
    let toggle = Rect::from_center_size(rect.right_center() - vec2(30.0, 0.0), vec2(34.0, 20.0));
    ui.painter().rect_filled(
        toggle,
        10.0,
        if value { theme::ACCENT } else { theme::BORDER },
    );
    ui.painter().circle_filled(
        toggle.center() + vec2(if value { 7.0 } else { -7.0 }, 0.0),
        7.0,
        if value { theme::SURFACE } else { theme::TEXT },
    );
    response
}
