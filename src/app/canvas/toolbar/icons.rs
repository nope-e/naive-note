//! Small vector icons, independent of the installed font's symbol coverage.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Check,
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
    None,
}

pub(super) fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    let origin = rect.center() - vec2(size, size) * 0.5;
    let point = |x: f32, y: f32| origin + vec2(x, y) * (size / 24.0);
    let stroke = Stroke::new(1.6, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let path = |points: &[(f32, f32)], closed: bool| {
        let points: Vec<Pos2> = points.iter().map(|&(x, y)| point(x, y)).collect();
        painter.add(if closed {
            egui::Shape::closed_line(points, stroke)
        } else {
            egui::Shape::line(points, stroke)
        });
    };
    match icon {
        Icon::Check => path(&[(5.0, 12.0), (10.0, 17.0), (19.0, 7.0)], false),
        Icon::Pages => {
            for x in [5.0, 13.0] {
                for y in [5.0, 13.0] {
                    painter.rect_filled(
                        Rect::from_two_pos(point(x, y), point(x + 5.0, y + 5.0)),
                        0.8,
                        color,
                    );
                }
            }
        }
        Icon::Eye => {
            path(
                &[
                    (3.0, 12.0),
                    (7.0, 7.0),
                    (12.0, 5.0),
                    (17.0, 7.0),
                    (21.0, 12.0),
                    (17.0, 17.0),
                    (12.0, 19.0),
                    (7.0, 17.0),
                ],
                true,
            );
            painter.circle_stroke(point(12.0, 12.0), size * 0.13, stroke);
            line((4.0, 21.0), (21.0, 3.0));
        }
        Icon::Download => {
            path(
                &[(4.0, 17.0), (4.0, 20.0), (20.0, 20.0), (20.0, 17.0)],
                false,
            );
            line((12.0, 3.0), (12.0, 15.0));
            path(&[(8.0, 11.0), (12.0, 15.0), (16.0, 11.0)], false);
        }
        Icon::Open => {
            path(
                &[
                    (14.0, 4.0),
                    (5.0, 4.0),
                    (5.0, 20.0),
                    (19.0, 20.0),
                    (19.0, 13.0),
                ],
                false,
            );
            path(&[(11.0, 13.0), (20.0, 4.0), (14.0, 4.0)], false);
            line((20.0, 4.0), (20.0, 10.0));
        }
        Icon::Menu => {
            for y in [6.0, 12.0, 18.0] {
                line((5.0, y), (19.0, y));
            }
        }
        Icon::Minimize | Icon::Minus => line((6.0, 12.0), (18.0, 12.0)),
        Icon::Plus => {
            line((6.0, 12.0), (18.0, 12.0));
            line((12.0, 6.0), (12.0, 18.0));
        }
        Icon::Maximize => {
            painter.rect_stroke(
                Rect::from_two_pos(point(6.0, 6.0), point(18.0, 18.0)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
        }
        Icon::Close => {
            line((7.0, 7.0), (17.0, 17.0));
            line((17.0, 7.0), (7.0, 17.0));
        }
        Icon::Pen => {
            path(
                &[
                    (5.0, 19.0),
                    (7.0, 13.0),
                    (17.0, 3.0),
                    (21.0, 7.0),
                    (11.0, 17.0),
                    (5.0, 19.0),
                ],
                false,
            );
            line((15.0, 5.0), (19.0, 9.0));
            line((7.0, 13.0), (11.0, 17.0));
            painter.circle_filled(point(5.0, 19.0), size * 0.09, color);
        }
        Icon::Marker => {
            path(
                &[(5.0, 15.0), (14.0, 4.0), (21.0, 10.0), (12.0, 20.0)],
                true,
            );
            line((7.0, 13.0), (14.0, 19.0));
            line((3.0, 21.0), (11.0, 21.0));
        }
        Icon::Text => {
            path(&[(4.0, 19.0), (10.0, 5.0), (16.0, 19.0)], false);
            line((7.0, 14.0), (13.0, 14.0));
            line((20.0, 5.0), (20.0, 19.0));
        }
        Icon::Eraser => {
            path(
                &[
                    (3.0, 14.0),
                    (12.0, 5.0),
                    (21.0, 14.0),
                    (15.0, 20.0),
                    (9.0, 20.0),
                ],
                true,
            );
            line((8.0, 9.0), (17.0, 18.0));
        }
        Icon::Select => {
            let dotted = Stroke::new(1.5, color);
            for i in 0..4 {
                let d = 5.0 + i as f32 * 4.0;
                for (a, b) in [
                    ((d, 5.0), (d + 2.0, 5.0)),
                    ((d, 19.0), (d + 2.0, 19.0)),
                    ((5.0, d), (5.0, d + 2.0)),
                    ((19.0, d), (19.0, d + 2.0)),
                ] {
                    painter.line_segment([point(a.0, a.1), point(b.0, b.1)], dotted);
                }
            }
        }
        Icon::Settings => {
            painter.circle_stroke(point(12.0, 12.0), size * 0.24, Stroke::new(2.5, color));
            painter.circle_stroke(point(12.0, 12.0), size * 0.06, stroke);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let direction = vec2(angle.cos(), angle.sin());
                painter.line_segment(
                    [
                        point(12.0, 12.0) + direction * size * 0.24,
                        point(12.0, 12.0) + direction * size * 0.35,
                    ],
                    Stroke::new(2.2, color),
                );
            }
        }
        Icon::Undo | Icon::Redo => {
            let mirror = |x: f32| {
                if matches!(icon, Icon::Redo) {
                    24.0 - x
                } else {
                    x
                }
            };
            path(
                &[(mirror(9.0), 6.0), (mirror(4.0), 11.0), (mirror(9.0), 16.0)],
                false,
            );
            path(
                &[
                    (mirror(4.0), 11.0),
                    (mirror(14.0), 11.0),
                    (mirror(18.0), 13.0),
                    (mirror(19.0), 17.0),
                ],
                false,
            );
        }
        Icon::Palette => {
            for (x, y, r) in [(9.0, 8.0, 4.0), (6.0, 16.0, 4.0), (16.0, 16.0, 4.0)] {
                painter.circle_stroke(point(x, y), r * size / 24.0, stroke);
            }
        }
        Icon::Previous => path(&[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)], false),
        Icon::Next => path(&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)], false),
        Icon::Hand => {
            path(
                &[
                    (6.0, 13.0),
                    (6.0, 7.0),
                    (9.0, 7.0),
                    (9.0, 3.0),
                    (12.0, 3.0),
                    (12.0, 9.0),
                    (12.0, 5.0),
                    (15.0, 5.0),
                    (15.0, 11.0),
                    (15.0, 8.0),
                    (18.0, 8.0),
                    (18.0, 16.0),
                    (15.0, 21.0),
                    (9.0, 21.0),
                    (3.0, 14.0),
                    (4.0, 12.0),
                    (9.0, 16.0),
                ],
                false,
            );
        }
        Icon::None => {}
    }
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
            ui.painter(),
            Rect::from_center_size(rect.center(), vec2(18.0, 18.0)),
            icon,
            color,
        );
        response
    })
    .inner
    .on_hover_text(label)
    .on_disabled_hover_text(format!("{label} · 暂未接入"))
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
