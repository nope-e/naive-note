use super::super::area::ZoomGesture;
use super::*;

#[test]
fn draft_starts_at_actual_size_and_explicit_fit_still_works() {
    let mut view = CanvasView::draft();
    let viewport =
        CanvasViewport::new(Rect::new(0.0, 96.0, 1280.0, 888.0), 1.0, (1280, 900)).unwrap();
    view.update_viewport(viewport);
    assert_eq!(view.transform.zoom(), 1.0);
    assert!(!view.fit_to_page);
    let size = view.page().paper_size();
    near(
        view.transform
            .paper_to_logical(Point::new(size.width * 0.5, 0.0)),
        Point::new(viewport.logical.center().x, viewport.logical.min.y + 24.0),
    );
    view.apply_action(ViewAction::Fit, viewport.logical);
    assert!(view.fit_to_page);
    assert!(view.transform.zoom() < 1.0);
}

fn setup() -> (CanvasView, CanvasViewport) {
    let mut view = CanvasView::new(vec![
        PageGeometry::new(Rect::new(20.0, 30.0, 220.0, 330.0), Rotation::R0, 1.5).unwrap(),
        PageGeometry::new(Rect::new(-10.0, 40.0, 390.0, 240.0), Rotation::R90, 2.0).unwrap(),
    ]);
    let viewport =
        CanvasViewport::new(Rect::new(0.0, 40.0, 800.0, 640.0), 1.5, (1200, 960)).unwrap();
    view.update_viewport(viewport);
    (view, viewport)
}

fn near(actual: Point, expected: Point) {
    assert!(
        (actual.x - expected.x).abs() < 1e-8 && (actual.y - expected.y).abs() < 1e-8,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn navigation_fits_target_page_and_maps_pointer_to_its_own_pdf_space() {
    let (mut view, viewport) = setup();
    view.apply_action(ViewAction::NextPage, viewport.logical);
    assert_eq!(view.current_page(), 1);
    let (index, paper, pdf) = view.pointer_coordinates(viewport.logical.center()).unwrap();
    assert_eq!(index, 1);
    near(paper, Point::new(200.0, 400.0));
    near(view.page().pdf_to_paper(pdf), paper);
    let scene = view
        .scenes(viewport, false)
        .into_iter()
        .find(|page| page.index == 1)
        .unwrap();
    near(
        scene.view.paper_to_logical(paper),
        viewport.logical.center(),
    );
    view.apply_action(ViewAction::GoToPage(u32::MAX), viewport.logical);
    assert_eq!(view.current_page(), 1);
    view.apply_action(ViewAction::PreviousPage, viewport.logical);
    view.apply_action(ViewAction::PreviousPage, viewport.logical);
    assert_eq!(view.current_page(), 0);
}

#[test]
fn scroll_tracks_current_page_and_clamps_at_document_ends() {
    let (mut view, viewport) = setup();
    for delta in [-100000.0, 100000.0] {
        view.apply_interaction(CanvasResponse {
            viewport,
            drag_delta: None,
            zoom: None,
            scroll_delta: Some(Point::new(0.0, delta)),
        });
        assert_eq!(view.current_page(), u32::from(delta < 0.0));
        assert!(!view.scenes(viewport, false).is_empty());
    }
}

#[test]
fn zoom_keeps_pointer_anchor_and_rotation_preserves_target_page() {
    let (mut view, viewport) = setup();
    view.apply_action(ViewAction::NextPage, viewport.logical);
    let anchor = Point::new(380.0, 360.0);
    let before = view.pointer_coordinates(anchor).unwrap();
    view.apply_interaction(CanvasResponse {
        viewport,
        drag_delta: None,
        zoom: Some(ZoomGesture {
            factor: 2.0,
            anchor,
        }),
        scroll_delta: None,
    });
    let after = view.pointer_coordinates(anchor).unwrap();
    assert_eq!(before.0, after.0);
    near(before.2, after.2);
    let zoom = view.transform.zoom();
    view.apply_action(ViewAction::GoToPage(0), viewport.logical);
    assert_eq!(view.transform.zoom(), zoom);
    view.apply_action(ViewAction::NextPage, viewport.logical);
    let geometry = view.page();
    for _ in 0..4 {
        view.apply_action(ViewAction::Rotate, viewport.logical);
    }
    assert_eq!(view.current_page(), 1);
    assert_eq!(view.page(), geometry);
}

#[test]
fn fit_width_and_resize_use_the_active_page() {
    let (mut view, viewport) = setup();
    view.apply_action(ViewAction::NextPage, viewport.logical);
    view.apply_action(ViewAction::FitWidth, viewport.logical);
    let scene = view
        .scenes(viewport, false)
        .into_iter()
        .find(|page| page.index == 1)
        .unwrap();
    near(
        scene.view.paper_to_logical(Point::default()),
        Point::new(24.0, 64.0),
    );
    let smaller =
        CanvasViewport::new(Rect::new(0.0, 60.0, 600.0, 600.0), 2.0, (1200, 1200)).unwrap();
    view.update_viewport(smaller);
    let scene = view
        .scenes(smaller, false)
        .into_iter()
        .find(|page| page.index == 1)
        .unwrap();
    near(
        scene.view.paper_to_logical(Point::default()),
        Point::new(24.0, 84.0),
    );
}

#[test]
fn scrollbar_and_wheel_reach_the_same_document_limits_at_all_zooms() {
    for zoom in [0.1, 1.0, 8.0] {
        let (mut view, viewport) = setup();
        view.transform.set_zoom_at(zoom, viewport.logical.center());
        let axes = view.scroll_axes(viewport.logical);
        for end in [false, true] {
            let offsets = axes.map(|axis| {
                // Deliberately overshoot: public scrollbar input must be clamped.
                Some(if end { axis.maximum + 1000.0 } else { -1000.0 })
            });
            view.apply_scroll_offsets(viewport.logical, offsets);
            let scrollbar_origin = view.transform.paper_to_logical(Point::default());
            assert_eq!(view.current_page(), u32::from(end));
            assert!(!view.fit_to_page && !view.fit_width);
            view.apply_interaction(CanvasResponse {
                viewport,
                drag_delta: None,
                zoom: None,
                scroll_delta: Some(Point::new(
                    if end { -1e6 } else { 1e6 },
                    if end { -1e6 } else { 1e6 },
                )),
            });
            near(
                view.transform.paper_to_logical(Point::default()),
                scrollbar_origin,
            );
            for axis in view.scroll_axes(viewport.logical) {
                assert!((axis.offset - if end { axis.maximum } else { 0.0 }).abs() < 1e-8);
            }
            assert!(!view.scenes(viewport, false).is_empty());
        }
    }
}

#[test]
fn scrollbar_ranges_follow_fit_zoom_rotation_and_dpi_independently() {
    for dpi in [1.0, 1.5, 2.0] {
        let mut view = CanvasView::default();
        let viewport = CanvasViewport::new(
            Rect::new(0.0, 40.0, 800.0, 640.0),
            dpi,
            ((800.0 * dpi) as u32, (640.0 * dpi) as u32),
        )
        .unwrap();
        view.update_viewport(viewport);
        assert!(
            view.scroll_axes(viewport.logical)
                .iter()
                .all(|axis| { axis.maximum.abs() < 1e-8 && axis.offset.abs() < 1e-8 })
        );
        view.transform.set_zoom_at(8.0, viewport.logical.center());
        assert!(
            view.scroll_axes(viewport.logical)
                .iter()
                .all(|axis| axis.maximum > 0.0 && axis.offset > 0.0)
        );
        view.apply_action(ViewAction::Rotate, viewport.logical);
        assert!(
            view.scroll_axes(viewport.logical)
                .iter()
                .all(|axis| { axis.maximum.abs() < 1e-8 && axis.offset.abs() < 1e-8 })
        );
    }
}

#[test]
fn scrollbar_tracks_page_navigation_and_ignores_absent_or_invalid_input() {
    let (mut view, viewport) = setup();
    let first_offset = view.scroll_axes(viewport.logical)[1].offset;
    view.apply_action(ViewAction::NextPage, viewport.logical);
    assert!(view.scroll_axes(viewport.logical)[1].offset > first_offset);
    let before = view.transform;
    for offsets in [[None, None], [Some(f64::NAN), Some(f64::INFINITY)]] {
        view.apply_scroll_offsets(viewport.logical, offsets);
        assert_eq!(view.transform, before);
        assert!(view.fit_to_page);
        assert_eq!(view.current_page(), 1);
    }
    let offset = view.scroll_axes(viewport.logical)[1].offset * 0.5;
    view.apply_scroll_offsets(viewport.logical, [None, Some(offset)]);
    assert!((view.scroll_axes(viewport.logical)[1].offset - offset).abs() < 1e-8);
    let center = view.transform.logical_to_paper(viewport.logical.center());
    let resized = CanvasViewport::new(Rect::new(0.0, 80.0, 600.0, 500.0), 1.0, (600, 500)).unwrap();
    view.update_viewport(resized);
    near(
        view.transform.paper_to_logical(center),
        resized.logical.center(),
    );
}
