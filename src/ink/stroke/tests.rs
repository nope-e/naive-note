use super::*;
use crate::document::{CanvasViewport, PageGeometry, Rect, Rotation, ViewTransform};

fn stroke(smoothing: bool) -> LiveStroke {
    let page = PageGeometry::new(Rect::new(0.0, 0.0, 600.0, 800.0), Rotation::R0, 1.0).unwrap();
    LiveStroke::new(
        StrokeTarget {
            page_index: 0,
            original: page,
            displayed: page,
            view: ViewTransform::default(),
            viewport: CanvasViewport::new(Rect::new(0.0, 0.0, 800.0, 600.0), 1.0, (800, 600))
                .unwrap(),
        },
        smoothing,
        PenStyle::default(),
    )
}

fn raw(phase: PenPhase, x: f64, y: f64, time_us: u64) -> RawSample {
    RawSample {
        position: Point::new(x, y),
        time_us,
        pressure: Some(0.7),
        tilt_degrees: Some([-25.0, 15.0]),
        rotation_degrees: Some(45.0),
        phase,
        native: None,
    }
}

fn finite(record: &StrokeRecord) {
    assert!(!record.outline.is_empty());
    assert!(record.modeled_samples.iter().all(|s| s.position.is_finite()
        && s.pressure.is_finite()
        && (0.0..=1.0).contains(&s.pressure)));
    for command in &record.outline {
        match command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => assert!(p.is_finite()),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => assert!(control1.is_finite() && control2.is_finite() && to.is_finite()),
            PathCommand::Close => {}
        }
    }
}

#[test]
fn smoothing_resamples_in_paper_space_and_preserves_raw_and_preset() {
    let mut modeled = stroke(true);
    let mut direct = stroke(false);
    let samples = vec![
        raw(PenPhase::Down, 10.0, 20.0, 1_000_000),
        raw(PenPhase::Move, 40.0, 55.0, 1_016_000),
        raw(PenPhase::Move, 60.0, 15.0, 1_032_000),
        raw(PenPhase::Up, 80.0, 40.0, 1_048_000),
    ];
    for sample in &samples {
        modeled.push(sample.clone());
        direct.push(sample.clone());
    }
    let modeled = modeled.finish().unwrap();
    let direct = direct.finish().unwrap();
    assert_eq!(modeled.raw_samples, samples);
    assert_eq!(direct.raw_samples, samples);
    assert!(
        modeled
            .processing
            .algorithm
            .starts_with("ink-stroke-modeler-rs@0.1.0")
    );
    let preset = modeled.processing.modeler.as_ref().unwrap();
    assert_eq!(preset.sampling_min_output_rate, 180.0);
    assert_eq!(preset.sampling_max_outputs_per_call, 200);
    assert!(modeled.modeled_samples.len() > direct.modeled_samples.len());
    assert_ne!(modeled.outline, direct.outline);
    assert_eq!(modeled.modeled_samples[0].time_us, 1_000_000);
    assert!(
        modeled
            .modeled_samples
            .windows(2)
            .all(|w| w[0].time_us < w[1].time_us)
    );
    assert_eq!(direct.processing.algorithm, "direct-round-outline");
    assert!(direct.processing.modeler.is_none());
    finite(&modeled);
    let mut bytes = Vec::new();
    ciborium::into_writer(modeled.as_ref(), &mut bytes).unwrap();
    let decoded: StrokeRecord = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(*modeled, decoded);
}

#[test]
fn predicted_tail_is_disposable_and_cannot_enter_confirmed_or_serialized_data() {
    let mut live = stroke(true);
    let down = raw(PenPhase::Down, 10.0, 20.0, 900_000);
    let moved = raw(PenPhase::Move, 60.0, 50.0, 908_000);
    live.push(down.clone());
    let before = live.snapshot();
    live.push(moved.clone());
    let preview = live.snapshot();
    assert!(!preview.predicted_outline.is_empty());
    assert_eq!(
        before.record.raw_samples.as_slice(),
        std::slice::from_ref(&down)
    );
    let confirmed = Arc::clone(&preview.record);
    // Reading a preview has no effect on confirmed modeling state.
    for _ in 0..5 {
        assert_eq!(live.snapshot().record, confirmed);
    }
    assert_eq!(
        confirmed.outline,
        super::super::round_outline(&confirmed.modeled_samples, &confirmed.style)
    );
    let up = raw(PenPhase::Up, 100.0, 10.0, 916_000);
    live.push(up.clone());
    assert!(live.snapshot().predicted_outline.is_empty());
    let finished = live.finish().unwrap();
    let mut reference = InkModeler::new();
    let expected: Vec<_> = [down, moved, up]
        .iter()
        .flat_map(|sample| reference.push(sample).unwrap())
        .collect();
    assert_eq!(finished.modeled_samples, expected);
    assert_eq!(
        preview.record, confirmed,
        "old preview remains immutable after Up"
    );
}

#[test]
fn taps_and_same_time_moves_keep_all_originals_and_finish_at_latest_endpoint() {
    for end in [Point::new(10.0, 20.0), Point::new(50.0, 60.0)] {
        let mut live = stroke(true);
        let mut samples = vec![
            raw(PenPhase::Down, 10.0, 20.0, 123),
            raw(PenPhase::Move, 20.0, 25.0, 123),
            raw(PenPhase::Up, end.x, end.y, 123),
        ];
        samples[0].pressure = None;
        samples[1].pressure = Some(0.0);
        for sample in &samples {
            live.push(sample.clone())
        }
        let record = live.finish().unwrap();
        assert!(record.processing.modeler.is_some());
        assert_eq!(record.raw_samples, samples);
        assert_eq!(record.modeled_samples[0].pressure, 0.5);
        let last = record.modeled_samples.last().unwrap();
        assert!((last.position.x - end.x).hypot(last.position.y - end.y) < 1.0);
        finite(&record);
    }
}

#[test]
fn large_absolute_clock_does_not_change_modeling_geometry() {
    let run = |origin: u64| {
        let mut live = stroke(true);
        live.push(raw(PenPhase::Down, 10.0, 20.0, origin));
        live.push(raw(PenPhase::Move, 60.0, 30.0, origin + 16_000));
        live.push(raw(PenPhase::Up, 90.0, 50.0, origin + 32_000));
        live.finish().unwrap()
    };
    let small = run(0);
    let origin = u64::MAX - 1_000_000;
    let large = run(origin);
    assert_eq!(small.outline, large.outline);
    for (small, large) in small.modeled_samples.iter().zip(&large.modeled_samples) {
        assert_eq!(large.time_us - origin, small.time_us);
    }
}

#[test]
fn modeler_failure_rebuilds_entire_raw_stroke_and_stays_direct() {
    for failed_time in [3_000_000, 900_000] {
        let mut live = stroke(true);
        let mut direct = stroke(false);
        let samples = [
            raw(PenPhase::Down, 10.0, 20.0, 1_000_000),
            raw(PenPhase::Move, 40.0, 55.0, 1_016_000),
            raw(PenPhase::Move, 90.0, 10.0, failed_time),
            raw(PenPhase::Up, 100.0, 50.0, failed_time + 16_000),
        ];
        for sample in &samples {
            live.push(sample.clone());
            direct.push(sample.clone());
        }
        assert!(live.snapshot().predicted_outline.is_empty());
        let live = live.finish().unwrap();
        let direct = direct.finish().unwrap();
        assert_eq!(live.raw_samples, samples);
        assert_eq!(live.modeled_samples, direct.modeled_samples);
        assert_eq!(live.outline, direct.outline);
        assert_eq!(live.processing, direct.processing);
        finite(&live);
    }
}

#[test]
fn finish_without_physical_up_completes_modeler_at_last_ink_sample() {
    let mut live = stroke(true);
    let samples = [
        raw(PenPhase::Down, 10.0, 20.0, 100),
        raw(PenPhase::Move, 60.0, 50.0, 16_100),
    ];
    for sample in &samples {
        live.push(sample.clone())
    }
    let live = live.finish().unwrap();
    assert_eq!(
        live.raw_samples, samples,
        "synthetic Up must not change device data"
    );
    let last = live.modeled_samples.last().unwrap().position;
    assert!((last.x - 60.0).hypot(last.y - 50.0) < 1.0);
    assert!(live.processing.modeler.is_some());
    finite(&live);
}

#[test]
fn malformed_input_cannot_poison_confirmed_or_predicted_geometry() {
    let mut live = stroke(true);
    live.push(raw(PenPhase::Down, 10.0, 20.0, 100));
    live.push(raw(PenPhase::Move, f64::NAN, 30.0, 200));
    let mut invalid = raw(PenPhase::Move, 15.0, 30.0, 300);
    invalid.pressure = Some(f64::INFINITY);
    live.push(invalid);
    live.push(raw(PenPhase::Up, 30.0, 40.0, 16_100));
    let live = live.finish().unwrap();
    assert_eq!(live.raw_samples.len(), 2);
    finite(&live);
}
