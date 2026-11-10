use super::*;

#[test]
fn old_pen_style_decodes_as_opaque_pen_and_highlighter_uses_pressure() {
    let old =
        r#"{"color_rgb":[1,2,3],"max_width_pt":4.0,"min_width_ratio":0.2,"pressure_gamma":1.0}"#;
    let style: PenStyle = serde_json::from_str(old).unwrap();
    assert_eq!(style.kind, InkKind::Pen);
    let style = PenStyle::highlighter();
    for (pressure, width) in [(0., 3.2), (0.25, 6.4), (0.5, 9.6), (1., 16.)] {
        assert!((style.width(pressure) - width).abs() < 1e-9);
    }
}

#[test]
fn old_processing_snapshots_decode_without_modeler_parameters() {
    #[derive(serde::Serialize)]
    struct LegacyProcessing {
        algorithm: String,
        version: u32,
        coordinate_unit: String,
    }
    let legacy = LegacyProcessing {
        algorithm: "direct-round-outline".to_owned(),
        version: 1,
        coordinate_unit: "paper-pt-top-left".to_owned(),
    };
    let mut bytes = Vec::new();
    ciborium::into_writer(&legacy, &mut bytes).unwrap();
    let decoded: ProcessingSnapshot = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(decoded.algorithm, legacy.algorithm);
    assert_eq!(decoded.modeler, None);
}

#[test]
fn old_version_one_raw_samples_without_native_attributes_still_decode() {
    #[derive(serde::Serialize)]
    struct LegacySample {
        position: Point,
        time_us: u64,
        pressure: Option<f64>,
        tilt_degrees: Option<[f64; 2]>,
        rotation_degrees: Option<f64>,
        phase: PenPhase,
    }
    let legacy = LegacySample {
        position: Point::new(1.0, 2.0),
        time_us: 7,
        pressure: None,
        tilt_degrees: Some([3.0, 4.0]),
        rotation_degrees: None,
        phase: PenPhase::Down,
    };
    let mut bytes = Vec::new();
    ciborium::into_writer(&legacy, &mut bytes).unwrap();
    let sample: RawSample = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(sample.native, None);
    assert_eq!(sample.tilt_degrees, legacy.tilt_degrees);
    assert_eq!(sample.time_us, 7);
    assert_eq!(sample.pressure, None);
}
#[test]
fn pressure_curve_presets_have_distinct_widths_and_fixed_mode_ignores_pressure() {
    let mut style = super::PenStyle {
        max_width_pt: 10.0,
        min_width_ratio: 0.2,
        ..Default::default()
    };
    for (gamma, expected) in [
        (0.0, 10.0),
        (1.0, 4.0),
        (0.5, 6.0),
        (1.0 / 3.0, 7.039_684_199_579_493),
        (2.0, 2.5),
        (3.0, 2.125),
    ] {
        style.pressure_gamma = gamma;
        assert!((style.width(0.25) - expected).abs() < 1e-9);
        assert!((style.width(1.0) - 10.0).abs() < 1e-9);
        assert!((style.width(0.0) - if gamma == 0.0 { 10.0 } else { 2.0 }).abs() < 1e-9);
    }
}
