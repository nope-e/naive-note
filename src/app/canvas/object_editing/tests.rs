use super::*;
use crate::{
    document::{CanvasViewport, PageGeometry, PenPhase, PenStyle, Rotation},
    ink::{LiveStroke, StrokeTarget},
    pdf::PdfWorker,
};
use std::sync::Arc;

fn fixture() -> (CanvasDocument, StrokeTarget) {
    let original = PageGeometry::new(Rect::new(20., 30., 620., 830.), Rotation::R0, 1.5).unwrap();
    let mut view = super::super::view::CanvasView::new(vec![original]);
    view.update_viewport(
        CanvasViewport::new(Rect::new(0., 0., 900., 700.), 1.5, (1350, 1050)).unwrap(),
    );
    let target = view.stroke_target(Point::new(675., 525.)).unwrap();
    (
        CanvasDocument::with_worker(PdfWorker::new(|_| {}).unwrap()),
        target,
    )
}

fn stroke(target: StrokeTarget, y: f64) -> Arc<crate::document::StrokeRecord> {
    let mut ink = LiveStroke::new(
        target,
        false,
        PenStyle {
            max_width_pt: 4.,
            min_width_ratio: 1.,
            ..Default::default()
        },
    );
    for (x, phase) in [(80., PenPhase::Down), (180., PenPhase::Up)] {
        let physical = target
            .viewport
            .logical_to_physical(target.view.paper_to_logical(Point::new(x, y)));
        ink.push_mouse(physical, 0, phase);
    }
    ink.finish().unwrap()
}

fn gesture(
    target: StrokeTarget,
    tool: CanvasTool,
    start: Point,
    end: Point,
    additive: bool,
) -> ObjectGesture {
    ObjectGesture {
        target,
        tool,
        start,
        previous: start,
        position: end,
        additive,
    }
}

#[test]
fn selection_topmost_toggle_box_and_cancel_preserve_document_revision() {
    let (mut document, target) = fixture();
    let a = stroke(target, 100.);
    let b = stroke(target, 100.);
    let c = stroke(target, 200.);
    for stroke in [&a, &b, &c] {
        document.add_stroke(stroke.clone());
    }
    let revision = document.revision();
    let mut editor = ObjectEditing::default();
    let p = Point::new(120., 100.);
    let click = gesture(target, CanvasTool::Select, p, p, false);
    editor.receive(
        vec![ObjectEvent::Update(click), ObjectEvent::Finish],
        &mut document,
    );
    assert_eq!(editor.selected, HashSet::from([b.id]));
    let mut toggle = click;
    toggle.additive = true;
    editor.receive(
        vec![ObjectEvent::Update(toggle), ObjectEvent::Finish],
        &mut document,
    );
    assert!(editor.selected.is_empty());
    let bounds = gesture(
        target,
        CanvasTool::Select,
        Point::new(70., 90.),
        Point::new(190., 210.),
        false,
    );
    editor.receive(
        vec![ObjectEvent::Update(bounds), ObjectEvent::Finish],
        &mut document,
    );
    assert_eq!(editor.selected, HashSet::from([a.id, b.id, c.id]));
    editor.receive(
        vec![ObjectEvent::Update(click), ObjectEvent::Cancel],
        &mut document,
    );
    assert_eq!(editor.selected.len(), 3);
    // A box that merely intersects the ink does not enclose its object.
    let partial = gesture(
        target,
        CanvasTool::Select,
        Point::new(90., 90.),
        Point::new(170., 110.),
        false,
    );
    editor.receive(
        vec![ObjectEvent::Update(partial), ObjectEvent::Finish],
        &mut document,
    );
    assert!(editor.selected.is_empty());
    assert_eq!(document.revision(), revision);
}

#[test]
fn eraser_sparse_sweep_is_atomic_cancelable_and_undo_restores_stacking() {
    let (mut document, target) = fixture();
    let a = stroke(target, 100.);
    let b = stroke(target, 150.);
    let c = stroke(target, 200.);
    for stroke in [&a, &b, &c] {
        document.add_stroke(stroke.clone());
    }
    let revision = document.revision();
    let mut editor = ObjectEditing::default();
    let sweep = gesture(
        target,
        CanvasTool::Eraser,
        Point::new(120., 80.),
        Point::new(120., 170.),
        false,
    );
    editor.receive(vec![ObjectEvent::Update(sweep)], &mut document);
    assert_eq!(editor.erased, HashSet::from([a.id, b.id]));
    assert_eq!(
        document.strokes(0).len(),
        3,
        "preview has no committed edits"
    );
    editor.receive(vec![ObjectEvent::Cancel], &mut document);
    assert!(editor.erased.is_empty());
    assert_eq!(document.revision(), revision);
    editor.receive(
        vec![ObjectEvent::Update(sweep), ObjectEvent::Finish],
        &mut document,
    );
    assert_eq!(document.strokes(0).len(), 1);
    assert_eq!(document.strokes(0)[0].id, c.id);
    document.undo();
    for (restored, original) in document.strokes(0).iter().zip([&a, &b, &c]) {
        assert!(Arc::ptr_eq(restored, original));
    }
    assert_eq!(document.revision(), revision);
    document.redo();
    assert_eq!(document.strokes(0).len(), 1);
    editor.select_page(&document, 0);
    editor.delete_selected(&mut document);
    assert!(document.strokes(0).is_empty());
    document.undo();
    assert_eq!(document.strokes(0)[0].id, c.id);
}

#[test]
fn page_clip_rejects_off_page_sweeps_and_clips_crossings() {
    let rect = Rect::new(0., 0., 100., 200.);
    assert!(clip_segment(Point::new(-10., 10.), Point::new(-10., 190.), rect).is_none());
    assert_eq!(
        clip_segment(Point::new(-20., 100.), Point::new(120., 100.), rect),
        Some((Point::new(0., 100.), Point::new(100., 100.)))
    );
}
