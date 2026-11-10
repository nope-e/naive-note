use super::*;
use crate::app::canvas::ink_input::{CanvasTool, ObjectEvent};
use crate::document::InkKind;

#[test]
fn pen_and_mouse_route_object_tools_without_ink_even_when_mouse_writing_is_off() {
    for mouse in [false, true] {
        for tool in [CanvasTool::Eraser, CanvasTool::Select, CanvasTool::Lasso] {
            let (mut input, mut view, position) = setup();
            input.tool = tool;
            input.mouse_pen = false;
            if mouse {
                assert!(
                    input
                        .mouse_button(
                            true,
                            Some(position),
                            &mut view,
                            true,
                            egui::Modifiers::SHIFT
                        )
                        .0
                );
                assert!(
                    input
                        .mouse_move(position, &mut view, egui::Modifiers::SHIFT)
                        .is_none()
                );
            } else {
                input.pen(
                    sample(PenPhase::Down, position),
                    &mut view,
                    true,
                    egui::Modifiers::SHIFT,
                );
                input.pen(
                    sample(PenPhase::Move, position),
                    &mut view,
                    true,
                    egui::Modifiers::SHIFT,
                );
            }
            assert!(input.is_drawing() && input.active().is_none());
            if mouse {
                assert!(
                    input
                        .mouse_button(
                            false,
                            Some(position),
                            &mut view,
                            true,
                            egui::Modifiers::SHIFT
                        )
                        .1
                        .is_none()
                );
            } else {
                assert!(
                    input
                        .pen(
                            sample(PenPhase::Up, position),
                            &mut view,
                            true,
                            egui::Modifiers::SHIFT
                        )
                        .1
                        .is_none()
                );
            }
            let events = input.take_object_events();
            assert!(matches!(events.last(), Some(ObjectEvent::Finish)));
            let ObjectEvent::Update(first) = events[0] else {
                panic!("missing object Down");
            };
            assert_eq!(first.tool, tool);
            assert!(first.additive);
            assert!(!input.is_drawing());
        }
    }
}

#[test]
fn object_tool_ui_ownership_navigation_and_cancel_never_emit_an_edit_commit() {
    for mouse in [false, true] {
        let (mut input, mut view, position) = setup();
        input.tool = CanvasTool::Eraser;
        input.ui_blocked = true;
        if mouse {
            input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
        } else {
            input.pen(
                sample(PenPhase::Down, position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
        }
        assert!(input.take_object_events().is_empty());
        input.cancel();
        input.ui_blocked = false;
        if mouse {
            input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
        } else {
            input.pen(
                sample(PenPhase::Down, position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
        }
        input.modifiers_changed(egui::Modifiers::CTRL, &mut view);
        assert!(!input.is_drawing());
        let events = input.take_object_events();
        assert!(matches!(events.last(), Some(ObjectEvent::Cancel)));
        if mouse {
            input.mouse_button(
                false,
                Some(position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
        } else {
            input.pen(
                sample(PenPhase::Up, position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
        }
        assert!(input.take_object_events().is_empty());
    }
}

#[test]
fn highlighter_keeps_independent_style_and_records_native_pressure_or_fixed_mouse_pressure() {
    for mouse in [false, true] {
        let (mut input, mut view, position) = setup();
        let pen = input.style.clone();
        input.tool = CanvasTool::Highlighter;
        input.mouse_pen = true;
        input.current_style_mut().color_rgb = [200, 100, 20];
        input.current_style_mut().max_width_pt = 20.;
        let stroke = if mouse {
            input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
            input
                .mouse_button(
                    false,
                    Some(position),
                    &mut view,
                    true,
                    egui::Modifiers::NONE,
                )
                .1
                .unwrap()
        } else {
            let mut down = sample(PenPhase::Down, position);
            down.pressure = Some(0.01);
            input.pen(down, &mut view, true, egui::Modifiers::NONE);
            let mut up = sample(PenPhase::Up, position);
            up.pressure = Some(1.0);
            input
                .pen(up, &mut view, true, egui::Modifiers::NONE)
                .1
                .unwrap()
        };
        assert_eq!(stroke.style.kind, InkKind::Highlighter);
        assert!((stroke.style.width(0.01) - 4.16).abs() < 1e-9);
        assert_eq!(stroke.style.width(1.), 20.);
        if mouse {
            assert!(stroke.modeled_samples.iter().all(|s| s.pressure == 0.5));
        } else {
            assert_eq!(stroke.raw_samples[0].pressure, Some(f64::from(0.01_f32)));
            assert_eq!(stroke.raw_samples[1].pressure, Some(1.));
        }
        input.tool = CanvasTool::Pen;
        assert_eq!(input.current_style(), &pen);
        assert!(input.take_object_events().is_empty());
    }
}
