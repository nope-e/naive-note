//! Scrollbar widgets only; document ranges and view updates belong to CanvasView.

use super::view::ScrollAxis;

#[derive(Default)]
pub(super) struct Scrollbars {
    grab_offsets: [Option<f32>; 2],
}

impl Scrollbars {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        tracks: [egui::Rect; 2],
        axes: [ScrollAxis; 2],
        process_input: bool,
    ) -> [Option<f64>; 2] {
        let mut offsets = [None; 2];
        for d in 0..2 {
            let track = tracks[d];
            let axis = axes[d];
            if axis.maximum <= 1e-8 || track.size()[d] <= 0.0 {
                if process_input {
                    self.grab_offsets[d] = None;
                }
                continue;
            }
            let response = ui.interact(
                track,
                ui.scope_id().with(("canvas_scrollbar", d)),
                egui::Sense::click_and_drag(),
            );
            response.widget_info(|| egui::WidgetInfo::new(egui::Role::ScrollBar));
            let length = track.size()[d];
            // Only the small, on-screen track uses f32. Document offsets stay
            // f64 so the thumb remains usable for long PDFs at high zoom.
            let handle_length = (length * (axis.visible / (axis.visible + axis.maximum)) as f32)
                .max(ui.spacing().scroll.handle_min_length)
                .min(length);
            let travel = length - handle_length;
            let handle_start = |offset: f64| {
                track.min[d] + travel * (offset / axis.maximum).clamp(0.0, 1.0) as f32
            };
            let handle_rect = |offset: f64| {
                let mut handle = track;
                handle.min[d] = handle_start(offset);
                handle.max[d] = handle.min[d] + handle_length;
                let inset = (track.size()[1 - d] * 0.2).min(3.0);
                handle.min[1 - d] += inset;
                handle.max[1 - d] -= inset;
                handle
            };
            let active = response.is_pointer_button_down_on()
                && ui.input(|input| {
                    input.raw.focused && input.pointer.button_down(egui::PointerButton::Primary)
                });
            if process_input {
                if (active || response.clicked_by(egui::PointerButton::Primary))
                    && let Some(pointer) = response.interact_pointer_pos()
                    && travel > 0.0
                {
                    let grab = *self.grab_offsets[d].get_or_insert_with(|| {
                        let press = ui
                            .input(|input| input.pointer.press_origin())
                            .unwrap_or(pointer);
                        let start = handle_start(axis.offset);
                        if (start..=start + handle_length).contains(&press[d]) {
                            press[d] - start
                        } else {
                            // Clicking the track centers the thumb at the pointer.
                            handle_length * 0.5
                        }
                    });
                    let fraction = ((pointer[d] - grab - track.min[d]) / travel).clamp(0.0, 1.0);
                    let offset = f64::from(fraction) * axis.maximum;
                    if (offset - axis.offset).abs() > 1e-6 {
                        offsets[d] = Some(offset);
                    }
                } else {
                    self.grab_offsets[d] = None;
                }
            }
            let color = if response.hovered() || active {
                crate::app::theme::MUTED
            } else {
                crate::app::theme::BORDER
            };
            ui.painter()
                .rect_filled(handle_rect(offsets[d].unwrap_or(axis.offset)), 3.0, color);
            response.on_hover_cursor(egui::CursorIcon::Default);
        }
        offsets
    }
}
