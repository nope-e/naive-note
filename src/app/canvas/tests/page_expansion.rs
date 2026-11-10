//! Input, geometry and history checks only; no windows or pixel assertions.

use super::*;
use crate::document::ReferencePattern;
use tablet_input_windows::{PenPhase, PenSample, PenTilt};

fn setup(rotation: Rotation, dpi: f64) -> CanvasWorkspace {
    let mut workspace = workspace();
    let page = PageGeometry::new(Rect::new(-20., 30., 580., 830.), rotation, 1.5).unwrap();
    workspace.apply_transition(DocumentAction::Create(page));
    workspace.view.update_viewport(
        CanvasViewport::new(Rect::new(0., 0., 1200., 1400.), dpi, (2400, 2800)).unwrap(),
    );
    workspace
        .view
        .apply_action(ViewAction::Fit, workspace.view.viewport.unwrap().logical);
    workspace.input.smoothing = false;
    workspace
}

use super::super::view::ViewAction;

fn physical(workspace: &CanvasWorkspace, page: u32, y: f64) -> Point {
    let viewport = workspace.view.viewport.unwrap();
    let scene = workspace
        .view
        .scenes(viewport, false)
        .into_iter()
        .find(|s| s.index == page)
        .unwrap();
    let size = scene.page.paper_size();
    viewport.logical_to_physical(
        scene
            .view
            .paper_to_logical(Point::new(size.width * 0.5, size.height * y)),
    )
}

fn sample(phase: PenPhase, position: Point) -> crate::platform::PenEvent {
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
    }
}

#[test]
fn mouse_crossing_expands_during_contact_without_moving_ink_and_is_undoable() {
    for dpi in [1.0, 1.5, 2.0] {
        let mut workspace = setup(Rotation::R0, dpi);
        let start = physical(&workspace, 0, 0.90);
        let end = physical(&workspace, 0, 0.96);
        let expected = workspace
            .view
            .stroke_target(start)
            .unwrap()
            .paper_position(end);
        let transform = workspace.view.transform;
        workspace.mouse_button(true, Some(start), egui::Modifiers::NONE, false);
        assert_eq!(workspace.view.page_count(), 1);
        workspace.mouse_move(end, egui::Modifiers::NONE);
        assert_eq!(
            workspace.view.page_count(),
            2,
            "expansion must happen before Up"
        );
        assert_eq!(workspace.view.transform, transform);
        assert_eq!(workspace.view.current_page(), 0);
        assert!(workspace.input.is_drawing());
        workspace.mouse_move(end, egui::Modifiers::NONE);
        workspace.mouse_button(false, Some(end), egui::Modifiers::NONE, false);
        assert_eq!(workspace.view.page_count(), 2);
        let stroke = &workspace.document.strokes(0)[0];
        assert_eq!(stroke.raw_samples.last().unwrap().position, expected);
        assert!(workspace.document.strokes(1).is_empty());
        let pages = workspace.document.pages(workspace.view.draft_page());
        assert_eq!(pages[0], pages[1]);
        workspace.document.undo(); // Completed ink, then the preceding page addition.
        workspace.document.undo();
        workspace.sync_document_pages();
        assert_eq!(workspace.view.page_count(), 1);
        assert!(workspace.document.strokes(0).is_empty());
        workspace.document.redo();
        workspace.document.redo();
        workspace.sync_document_pages();
        assert_eq!(workspace.view.page_count(), 2);
        assert_eq!(workspace.document.strokes(0).len(), 1);
    }
}

#[test]
fn native_down_and_up_expand_in_all_rotations_and_guide_moves_to_the_last_page() {
    for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
        for reading_rotation in [false, true] {
            let mut workspace = setup(rotation, 1.5);
            if reading_rotation {
                workspace
                    .view
                    .apply_action(ViewAction::Rotate, workspace.view.viewport.unwrap().logical);
            }
            let end = physical(&workspace, 0, 0.96);
            let transform = workspace.view.transform;
            workspace.pen_sample(sample(PenPhase::Hover, end), false);
            assert_eq!(workspace.view.page_count(), 1);
            workspace.pen_sample(sample(PenPhase::Down, end), false);
            assert_eq!(workspace.view.page_count(), 2);
            assert_eq!(workspace.view.transform, transform);
            workspace.pen_sample(sample(PenPhase::Move, end), false);
            workspace.pen_sample(sample(PenPhase::Up, end), false);
            assert_eq!(workspace.view.page_count(), 2);
            assert_eq!(workspace.document.strokes(0).len(), 1);
            workspace.view.go_to_page(1);
            let scenes = workspace
                .view
                .scenes(workspace.view.viewport.unwrap(), false);
            assert!(
                scenes
                    .iter()
                    .filter(|s| s.index == 0)
                    .all(|s| s.page_end_y.is_none())
            );
            let last = scenes.iter().find(|s| s.index == 1).unwrap();
            assert_eq!(
                last.page_end_y,
                Some(last.page.paper_size().height * 15.0 / 16.0)
            );
            let end = physical(&workspace, 1, 0.96);
            workspace.pen_sample(sample(PenPhase::Down, end), false);
            assert_eq!(workspace.view.page_count(), 3);
            workspace.pen_sample(sample(PenPhase::Up, end), false);
        }
    }
    // A final Up sample can be the first sample below the guide.
    let mut workspace = setup(Rotation::R0, 1.0);
    let start = physical(&workspace, 0, 0.90);
    let end = physical(&workspace, 0, 0.96);
    workspace.pen_sample(sample(PenPhase::Down, start), false);
    workspace.pen_sample(sample(PenPhase::Up, end), false);
    assert_eq!(workspace.view.page_count(), 2);
}

#[test]
fn navigation_object_tools_ui_and_disabled_aid_do_not_expand() {
    use super::super::ink_input::CanvasTool;
    for tool in [CanvasTool::Pan, CanvasTool::Select, CanvasTool::Eraser] {
        let mut workspace = setup(Rotation::R0, 1.0);
        workspace.input.tool = tool;
        let end = physical(&workspace, 0, 0.96);
        workspace.mouse_button(true, Some(end), egui::Modifiers::NONE, false);
        workspace.mouse_button(false, Some(end), egui::Modifiers::NONE, false);
        assert_eq!(workspace.view.page_count(), 1);
    }
    let mut workspace = setup(Rotation::R0, 1.0);
    let end = physical(&workspace, 0, 0.96);
    workspace.pen_sample(sample(PenPhase::Down, end), true);
    workspace.pen_sample(sample(PenPhase::Up, end), true);
    assert_eq!(workspace.view.page_count(), 1);
    let mut layer = workspace.document.reference_layer();
    layer.auto_expand = false;
    workspace.document.set_reference_layer(layer);
    workspace.mouse_button(true, Some(end), egui::Modifiers::NONE, false);
    workspace.mouse_button(false, Some(end), egui::Modifiers::NONE, false);
    assert_eq!(workspace.view.page_count(), 1);
    assert_eq!(workspace.document.strokes(0).len(), 1);
    let context = egui::Context::default();
    let mut scene = CanvasScene {
        viewport: workspace.view.viewport.unwrap(),
        pages: workspace
            .view
            .scenes(workspace.view.viewport.unwrap(), false),
    };
    workspace.document.update_scene(&context, &mut scene, 0);
    assert!(scene.pages.iter().all(|page| page.page_end_y.is_none()));
}

#[test]
fn highlighter_expands_even_when_the_reference_pattern_is_none() {
    let mut workspace = setup(Rotation::R0, 1.0);
    let mut layer = workspace.document.reference_layer();
    layer.lines.pattern = ReferencePattern::None;
    workspace.document.set_reference_layer(layer);
    workspace.input.tool = super::super::ink_input::CanvasTool::Highlighter;
    let end = physical(&workspace, 0, 0.96);
    workspace.mouse_button(true, Some(end), egui::Modifiers::NONE, false);
    assert_eq!(workspace.view.page_count(), 2);
    workspace.mouse_button(false, Some(end), egui::Modifiers::NONE, false);
    // Going back to the old page's bottom must not add another page.
    workspace.mouse_button(true, Some(end), egui::Modifiers::NONE, false);
    workspace.mouse_button(false, Some(end), egui::Modifiers::NONE, false);
    assert_eq!(workspace.view.page_count(), 2);
}
