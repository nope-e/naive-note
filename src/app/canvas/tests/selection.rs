//! Device routing and page-space transformations only. Never opens a window.

use super::super::{ink_input::CanvasTool, view::ViewAction};
use super::*;
use crate::{document::InkTransform, ink::StrokeTarget};
use tablet_input_windows::{PenPhase, PenSample, PenTilt};

mod cross_page;

fn physical(target: StrokeTarget, paper: Point) -> Point {
    let displayed = target
        .displayed
        .pdf_to_paper(target.original.paper_to_pdf(paper));
    target
        .viewport
        .logical_to_physical(target.view.paper_to_logical(displayed))
}

fn contact(workspace: &mut CanvasWorkspace, mouse: bool, phase: PenPhase, position: Point) {
    if mouse {
        if phase == PenPhase::Move {
            workspace.mouse_move(position, egui::Modifiers::NONE);
        } else {
            assert!(workspace.mouse_button(
                phase == PenPhase::Down,
                Some(position),
                egui::Modifiers::NONE,
                false
            ));
        }
    } else {
        assert!(!workspace.pen_sample(
            crate::platform::PenEvent {
                sample: PenSample {
                    pointer_id: 4,
                    stroke_id: Some(7),
                    time_us: 123_456,
                    position: [position.x, position.y],
                    pressure: Some(0.5),
                    pressure_level: Some(512),
                    tilt_deg: PenTilt { x: None, y: None },
                    rotation_deg: None,
                    eraser: false,
                    inverted: false,
                    barrel_button: false,
                    phase,
                },
                modifiers: egui::Modifiers::NONE,
            },
            false
        ));
    }
}

fn close(a: Point, b: Point) {
    assert!(
        (a.x - b.x).abs() < 1e-7 && (a.y - b.y).abs() < 1e-7,
        "{a:?} != {b:?}"
    );
}

#[test]
fn cancel_save_and_modifier_navigation_drop_rotation_preview_for_both_devices() {
    for mouse in [false, true] {
        for cancel_mode in 0..3 {
            let mut workspace = workspace();
            let viewport =
                CanvasViewport::new(Rect::new(0., 0., 900., 800.), 1., (900, 800)).unwrap();
            workspace.view.update_viewport(viewport);
            workspace
                .view
                .apply_action(ViewAction::Fit, viewport.logical);
            let original = Arc::new(
                crate::ink::test_support::pressure_stroke(workspace.view.draft_page()).unwrap(),
            );
            workspace.document.add_stroke(original.clone());
            let revision = workspace.document.revision();
            workspace.input.tool = CanvasTool::Lasso;
            workspace.objects.select_page(&workspace.document, 0);
            let mut scene = CanvasScene {
                viewport,
                pages: workspace.view.scenes(viewport, true),
            };
            scene.pages[0].strokes = workspace.document.strokes(0).to_vec();
            workspace.objects.decorate(&mut scene);
            let handle = scene.pages[0]
                .selection_overlay
                .as_ref()
                .unwrap()
                .rotation_handle;
            let target = workspace
                .view
                .stroke_target(viewport.logical.center())
                .unwrap();
            let end = InkTransform {
                center: original.bounds().center(),
                radians: 0.4,
                ..Default::default()
            }
            .apply(handle);
            contact(
                &mut workspace,
                mouse,
                PenPhase::Down,
                physical(target, handle),
            );
            contact(&mut workspace, mouse, PenPhase::Move, physical(target, end));
            assert!(workspace.input.is_drawing());
            match cancel_mode {
                0 => workspace.cancel_input(),
                1 => workspace.request_save(false),
                _ => workspace.modifiers_changed(egui::Modifiers::CTRL),
            }
            assert!(!workspace.input.is_drawing());
            assert_eq!(workspace.document.revision(), revision);
            assert!(Arc::ptr_eq(&workspace.document.strokes(0)[0], &original));
            if mouse {
                workspace.mouse_button(
                    false,
                    Some(physical(target, end)),
                    egui::Modifiers::NONE,
                    false,
                );
            } else {
                contact(&mut workspace, mouse, PenPhase::Up, physical(target, end));
            }
            assert_eq!(workspace.document.revision(), revision);
            assert_eq!(workspace.objects.selected_count(), 1);
            if cancel_mode == 1 {
                assert!(workspace.take_save_request().is_some());
            }
        }
    }
}

#[test]
fn pen_and_mouse_move_and_rotate_selection_across_source_view_rotation_zoom_and_dpi() {
    for mouse in [false, true] {
        for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            for turns in 0..4 {
                for dpi in [1., 1.5, 2.] {
                    let mut workspace = workspace();
                    let original =
                        PageGeometry::new(Rect::new(-20., 30., 580., 830.), rotation, 1.5).unwrap();
                    workspace.apply_transition(DocumentAction::Create(original));
                    let viewport = CanvasViewport::new(
                        Rect::new(0., 0., 1200., 1400.),
                        dpi,
                        ((1200. * dpi) as u32, (1400. * dpi) as u32),
                    )
                    .unwrap();
                    workspace.view.update_viewport(viewport);
                    workspace
                        .view
                        .apply_action(ViewAction::Fit, viewport.logical);
                    for _ in 0..turns {
                        workspace
                            .view
                            .apply_action(ViewAction::Rotate, viewport.logical);
                    }
                    workspace.view.apply_action(
                        ViewAction::Zoom(if turns % 2 == 0 { 0.5 } else { 2. }),
                        viewport.logical,
                    );
                    let target = workspace
                        .view
                        .stroke_target(viewport.logical_to_physical(viewport.logical.center()))
                        .unwrap();
                    let size = original.paper_size();
                    let center = Point::new(size.width * 0.5, size.height * 0.5);
                    let mut ink = crate::ink::LiveStroke::new(
                        target,
                        false,
                        crate::document::PenStyle::default(),
                    );
                    ink.push_mouse(
                        physical(target, Point::new(center.x - 20., center.y)),
                        0,
                        crate::document::PenPhase::Down,
                    );
                    ink.push_mouse(
                        physical(target, Point::new(center.x + 20., center.y)),
                        1,
                        crate::document::PenPhase::Up,
                    );
                    let original = ink.finish().unwrap();
                    workspace.document.add_stroke(original.clone());
                    let revision = workspace.document.revision();
                    let view_before = workspace.view.transform;
                    workspace.input.tool = CanvasTool::Lasso;
                    workspace.input.mouse_pen = false;
                    let start = physical(target, center);
                    contact(&mut workspace, mouse, PenPhase::Down, start);
                    contact(&mut workspace, mouse, PenPhase::Up, start);
                    assert_eq!(workspace.objects.selected_count(), 1);
                    assert_eq!(workspace.document.revision(), revision);
                    let delta = Point::new(18., 23.);
                    let end = physical(target, Point::new(center.x + delta.x, center.y + delta.y));
                    contact(&mut workspace, mouse, PenPhase::Down, start);
                    contact(&mut workspace, mouse, PenPhase::Move, end);
                    assert!(Arc::ptr_eq(&workspace.document.strokes(0)[0], &original));
                    contact(&mut workspace, mouse, PenPhase::Up, end);
                    let moved = workspace.document.strokes(0)[0].clone();
                    close(
                        moved.modeled_samples[0].position,
                        Point::new(
                            original.modeled_samples[0].position.x + delta.x,
                            original.modeled_samples[0].position.y + delta.y,
                        ),
                    );
                    let mut scene = CanvasScene {
                        viewport,
                        pages: workspace.view.scenes(viewport, true),
                    };
                    scene.pages[0].strokes = workspace.document.strokes(0).to_vec();
                    workspace.objects.decorate(&mut scene);
                    let overlay = scene.pages[0].selection_overlay.as_ref().unwrap();
                    let pivot = moved.bounds().center();
                    let rotation = InkTransform {
                        center: pivot,
                        radians: std::f64::consts::FRAC_PI_2,
                        ..Default::default()
                    };
                    let handle = overlay.rotation_handle;
                    let rotated_handle = rotation.apply(handle);
                    contact(
                        &mut workspace,
                        mouse,
                        PenPhase::Down,
                        physical(target, handle),
                    );
                    contact(
                        &mut workspace,
                        mouse,
                        PenPhase::Move,
                        physical(target, rotated_handle),
                    );
                    assert!(Arc::ptr_eq(&workspace.document.strokes(0)[0], &moved));
                    contact(
                        &mut workspace,
                        mouse,
                        PenPhase::Up,
                        physical(target, rotated_handle),
                    );
                    let rotated = &workspace.document.strokes(0)[0];
                    close(
                        rotated.modeled_samples[0].position,
                        rotation.apply(moved.modeled_samples[0].position),
                    );
                    assert_eq!(rotated.raw_samples, original.raw_samples);
                    assert_eq!(workspace.document.revision().current, revision.current + 2);
                    assert_eq!(workspace.view.transform, view_before);
                    workspace.document.undo();
                    assert!(Arc::ptr_eq(&workspace.document.strokes(0)[0], &moved));
                    workspace.document.undo();
                    assert_eq!(workspace.document.revision(), revision);
                    assert!(Arc::ptr_eq(&workspace.document.strokes(0)[0], &original));
                }
            }
        }
    }
}
