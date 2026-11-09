use super::*;

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
