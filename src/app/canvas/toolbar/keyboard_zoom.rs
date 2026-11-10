//! Preserve short tablet-driver key taps and every zoom step between frames.

use crate::document::{MAX_ZOOM, MIN_ZOOM};

pub(super) fn factor(events: &[egui::Event], zoom: f64) -> Option<f64> {
    let mut target = zoom;
    let mut handled = false;
    for event in events {
        let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = event
        else {
            continue;
        };
        // Drivers may press and release Ctrl within one frame. The modifier
        // snapshot on this key event remains valid after Ctrl has been released.
        if !modifiers.ctrl || modifiers.alt {
            continue;
        }
        let step = match key {
            egui::Key::Equals | egui::Key::Plus => 1.2,
            egui::Key::Minus => 1.0 / 1.2,
            _ => continue,
        };
        // Apply the limits per step so reversing at a limit behaves the same
        // whether these key presses arrive in one frame or separate frames.
        target = (target * step).clamp(MIN_ZOOM, MAX_ZOOM);
        handled = true;
    }
    handled.then_some(target / zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    fn tap(key_code: egui::Key) -> [egui::Event; 4] {
        [
            egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
            key(key_code, true, egui::Modifiers::CTRL),
            key(key_code, false, egui::Modifiers::CTRL),
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ]
    }

    fn near(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn rapid_tablet_taps_keep_modifiers_and_every_step_after_ctrl_release() {
        for (key_code, expected) in [
            (egui::Key::Equals, 1.2_f64.powi(3)),
            (egui::Key::Plus, 1.2_f64.powi(3)),
            (egui::Key::Minus, 1.2_f64.powi(-3)),
        ] {
            let events: Vec<_> = (0..3).flat_map(|_| tap(key_code)).collect();
            near(factor(&events, 1.0).unwrap(), expected);
        }
    }

    #[test]
    fn only_ctrl_key_downs_count_and_shifted_plus_and_repeat_work() {
        let ignored = [
            key(egui::Key::Equals, true, egui::Modifiers::NONE),
            key(egui::Key::Minus, false, egui::Modifiers::CTRL),
            key(
                egui::Key::Plus,
                true,
                egui::Modifiers::CTRL | egui::Modifiers::ALT,
            ),
        ];
        assert!(factor(&ignored, 1.0).is_none());
        assert!(factor(&[], 1.0).is_none());

        let mut events = ignored.to_vec();
        events.push(key(
            egui::Key::Plus,
            true,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        ));
        let mut repeat = key(egui::Key::Equals, true, egui::Modifiers::CTRL);
        if let egui::Event::Key { repeat, .. } = &mut repeat {
            *repeat = true;
        }
        events.push(repeat);
        near(factor(&events, 1.0).unwrap(), 1.44);
    }

    #[test]
    fn batched_direction_changes_obey_each_step_at_both_zoom_limits() {
        let events: Vec<_> = tap(egui::Key::Equals)
            .into_iter()
            .chain(tap(egui::Key::Equals))
            .chain(tap(egui::Key::Minus))
            .collect();
        near(7.9 * factor(&events, 7.9).unwrap(), MAX_ZOOM / 1.2);

        let events: Vec<_> = tap(egui::Key::Minus)
            .into_iter()
            .chain(tap(egui::Key::Equals))
            .collect();
        near(MIN_ZOOM * factor(&events, MIN_ZOOM).unwrap(), 0.12);
    }
}
