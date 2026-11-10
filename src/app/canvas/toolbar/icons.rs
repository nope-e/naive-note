//! Embedded Lucide SVG icons; egui caches textures at the current DPI.

use egui::{Color32, Rect, Stroke, StrokeKind, pos2, vec2};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Check,
    New,
    Save,
    Print,
    Restore,
    Pages,
    Eye,
    Download,
    Open,
    Menu,
    Minimize,
    Maximize,
    Close,
    Pen,
    Marker,
    Text,
    Eraser,
    Select,
    Settings,
    Undo,
    Redo,
    Plus,
    Minus,
    Palette,
    Previous,
    Next,
    Hand,
    RotateClockwise,
    None,
}

pub(super) fn paint(ui: &egui::Ui, rect: Rect, icon: Icon, color: Color32) {
    let source = match icon {
        Icon::Check => egui::include_image!("../../../../assets/icons/lucide/check.svg"),
        Icon::New => egui::include_image!("../../../../assets/icons/lucide/file-plus.svg"),
        Icon::Save => egui::include_image!("../../../../assets/icons/lucide/save.svg"),
        Icon::Print => egui::include_image!("../../../../assets/icons/lucide/printer.svg"),
        Icon::Restore => egui::include_image!("../../../../assets/icons/lucide/copy.svg"),
        Icon::Pages => egui::include_image!("../../../../assets/icons/lucide/panel-left.svg"),
        Icon::Eye => egui::include_image!("../../../../assets/icons/lucide/eye-off.svg"),
        Icon::Download => egui::include_image!("../../../../assets/icons/lucide/download.svg"),
        Icon::Open => egui::include_image!("../../../../assets/icons/lucide/folder-open.svg"),
        Icon::Menu => egui::include_image!("../../../../assets/icons/lucide/menu.svg"),
        Icon::Maximize => egui::include_image!("../../../../assets/icons/lucide/square.svg"),
        Icon::Close => egui::include_image!("../../../../assets/icons/lucide/x.svg"),
        Icon::Pen => egui::include_image!("../../../../assets/icons/lucide/pencil-line.svg"),
        Icon::Marker => egui::include_image!("../../../../assets/icons/lucide/highlighter.svg"),
        Icon::Text => egui::include_image!("../../../../assets/icons/lucide/type.svg"),
        Icon::Eraser => egui::include_image!("../../../../assets/icons/lucide/eraser.svg"),
        Icon::Select => egui::include_image!("../../../../assets/icons/lucide/square-dashed.svg"),
        Icon::Settings => egui::include_image!("../../../../assets/icons/lucide/settings.svg"),
        Icon::Undo => egui::include_image!("../../../../assets/icons/lucide/undo-2.svg"),
        Icon::Redo => egui::include_image!("../../../../assets/icons/lucide/redo-2.svg"),
        Icon::Plus => egui::include_image!("../../../../assets/icons/lucide/plus.svg"),
        Icon::Palette => egui::include_image!("../../../../assets/icons/lucide/palette.svg"),
        Icon::Previous => egui::include_image!("../../../../assets/icons/lucide/chevron-left.svg"),
        Icon::Next => egui::include_image!("../../../../assets/icons/lucide/chevron-right.svg"),
        Icon::Hand => egui::include_image!("../../../../assets/icons/lucide/hand.svg"),
        Icon::RotateClockwise => {
            egui::include_image!("../../../../assets/icons/lucide/rotate-cw.svg")
        }
        Icon::Minimize | Icon::Minus => {
            egui::include_image!("../../../../assets/icons/lucide/minus.svg")
        }
        Icon::None => return,
    };
    // White source strokes allow tinting without keeping a texture per color.
    egui::Image::new(source).tint(color).paint_at(ui, rect);
}

pub(super) fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        let fill = if selected || response.is_pointer_button_down_on() {
            crate::app::theme::SELECTED
        } else if response.hovered() {
            crate::app::theme::HEADER
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 6.0, fill);
        if selected {
            ui.painter().rect_stroke(
                rect.shrink(0.5),
                6.0,
                Stroke::new(1.0, crate::app::theme::ACCENT.gamma_multiply(0.55)),
                StrokeKind::Inside,
            );
        }
        let color = if enabled {
            crate::app::theme::TEXT
        } else {
            Color32::from_rgb(125, 125, 138)
        };
        paint(
            ui,
            Rect::from_center_size(rect.center(), vec2(20.0, 20.0)),
            icon,
            color,
        );
        response
    })
    .inner
    .on_hover_text(label)
    .on_disabled_hover_text(match icon {
        Icon::Undo => "当前没有可撤销的会话编辑".to_owned(),
        Icon::Redo => "当前没有可重做的会话编辑".to_owned(),
        _ => format!("{label} · 暂未接入"),
    })
}

pub(super) fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(7.0, 28.0), egui::Sense::hover());
    ui.painter().line_segment(
        [
            pos2(rect.center().x, rect.top() + 3.0),
            pos2(rect.center().x, rect.bottom() - 3.0),
        ],
        Stroke::new(1.0, crate::app::theme::BORDER),
    );
}
