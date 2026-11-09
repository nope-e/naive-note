//! Tool routing with synthetic contacts only; no windows or egui frames.

use super::*;
use crate::app::canvas::{ink_input::CanvasTool, view::ViewAction};

fn canvas_contact(
    input: &mut CanvasInput,
    view: &mut CanvasView,
    mouse: bool,
    phase: PenPhase,
    position: Point,
    modifiers: egui::Modifiers,
) -> Option<std::sync::Arc<StrokeRecord>> {
    if mouse {
        if phase == PenPhase::Move {
            input.mouse_move(position, view, modifiers)
        } else {
            let (owned, stroke) = input.mouse_button(
                phase == PenPhase::Down,
                Some(position),
                view,
                true,
                modifiers,
            );
            assert!(owned);
            stroke
        }
    } else {
        let (ui_owned, stroke) = input.pen(sample(phase, position), view, true, modifiers);
        assert!(!ui_owned);
        stroke
    }
}

#[test]
fn selected_pan_moves_pen_and_mouse_at_each_dpi_then_pen_tool_restores_ink() {
    for mouse in [false, true] {
        for dpi in [1.0, 1.5, 2.0] {
            let (mut input, mut view, _) = setup();
            let viewport = CanvasViewport::new(
                view.viewport.unwrap().logical,
                dpi,
                ((800.0 * dpi) as u32, (640.0 * dpi) as u32),
            )
            .unwrap();
            view.update_viewport(viewport);
            view.apply_action(ViewAction::Zoom(4.0), viewport.logical);
            let position = viewport.logical_to_physical(viewport.logical.center());
            let origin = view.transform.paper_to_logical(Point::default());
            let zoom = view.transform.zoom();
            input.tool = CanvasTool::Pan;
            // Pen navigation also works when mouse writing is disabled; the
            // selected pan tool overrides enabled mouse writing as well.
            input.mouse_pen = mouse;
            for (phase, offset) in [
                (PenPhase::Down, Point::default()),
                (PenPhase::Move, Point::new(20.0, 30.0)),
                (PenPhase::Up, Point::new(30.0, 35.0)),
            ] {
                let point = Point::new(position.x + offset.x * dpi, position.y + offset.y * dpi);
                assert!(
                    canvas_contact(
                        &mut input,
                        &mut view,
                        mouse,
                        phase,
                        point,
                        egui::Modifiers::NONE,
                    )
                    .is_none()
                );
                assert!(!input.is_drawing());
                let after = view.transform.paper_to_logical(Point::default());
                assert!((after.x - origin.x - offset.x).abs() < 1e-8);
                assert!((after.y - origin.y - offset.y).abs() < 1e-8);
                assert_eq!(view.transform.zoom(), zoom);
            }
            input.tool = CanvasTool::Pen;
            assert!(
                canvas_contact(
                    &mut input,
                    &mut view,
                    mouse,
                    PenPhase::Down,
                    position,
                    egui::Modifiers::NONE,
                )
                .is_none()
            );
            assert!(input.is_drawing());
            let stroke = canvas_contact(
                &mut input,
                &mut view,
                mouse,
                PenPhase::Up,
                position,
                egui::Modifiers::NONE,
            )
            .unwrap();
            assert_eq!(stroke.raw_samples.len(), 2);
            assert!(!input.is_drawing());
        }
    }
}

#[test]
fn choosing_pan_during_ui_contact_preserves_ui_ownership_until_release() {
    for mouse in [false, true] {
        let (mut input, mut view, position) = setup();
        input.mouse_pen = true;
        input.ui_blocked = true;
        let before = view.transform;
        if mouse {
            assert!(
                !input
                    .mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE)
                    .0
            );
        } else {
            assert!(
                input
                    .pen(
                        sample(PenPhase::Down, position),
                        &mut view,
                        true,
                        egui::Modifiers::NONE
                    )
                    .0
            );
        }
        input.cancel_drawing();
        input.tool = CanvasTool::Pan;
        input.ui_blocked = false;
        let end = Point::new(position.x + 30.0, position.y + 30.0);
        if mouse {
            assert!(
                input
                    .mouse_move(end, &mut view, egui::Modifiers::NONE)
                    .is_none()
            );
            let (owned, stroke) =
                input.mouse_button(false, Some(end), &mut view, true, egui::Modifiers::NONE);
            assert!(!owned && stroke.is_none());
        } else {
            for phase in [PenPhase::Move, PenPhase::Up] {
                let (ui_owned, stroke) =
                    input.pen(sample(phase, end), &mut view, true, egui::Modifiers::NONE);
                assert!(ui_owned && stroke.is_none());
            }
        }
        assert_eq!(view.transform, before);
        assert!(!input.is_drawing());
    }
}

#[test]
fn selected_pan_keeps_ctrl_zoom_and_resumes_pan_without_a_jump() {
    for mouse in [false, true] {
        let (mut input, mut view, position) = setup();
        let viewport = view.viewport.unwrap();
        view.apply_action(ViewAction::Zoom(4.0), viewport.logical);
        input.tool = CanvasTool::Pan;
        let zoom = view.transform.zoom();
        let moved = Point::new(position.x, position.y - 75.0);
        for (phase, point) in [(PenPhase::Down, position), (PenPhase::Move, moved)] {
            assert!(
                canvas_contact(
                    &mut input,
                    &mut view,
                    mouse,
                    phase,
                    point,
                    egui::Modifiers::CTRL
                )
                .is_none()
            );
        }
        assert!((view.transform.zoom() / zoom - 0.25_f64.exp()).abs() < 1e-8);
        let before_release = view.transform;
        assert!(
            input
                .modifiers_changed(egui::Modifiers::NONE, &mut view)
                .is_none()
        );
        assert_eq!(view.transform, before_release);
        let origin = view.transform.paper_to_logical(Point::default());
        let end = Point::new(moved.x + 30.0, moved.y + 15.0);
        for phase in [PenPhase::Move, PenPhase::Up] {
            assert!(
                canvas_contact(
                    &mut input,
                    &mut view,
                    mouse,
                    phase,
                    end,
                    egui::Modifiers::NONE
                )
                .is_none()
            );
        }
        let after = view.transform.paper_to_logical(Point::default());
        assert!((after.x - origin.x - 20.0).abs() < 1e-8);
        assert!((after.y - origin.y - 10.0).abs() < 1e-8);
        assert_eq!(view.transform.zoom(), before_release.zoom());
        assert!(!input.is_drawing());
    }
}
