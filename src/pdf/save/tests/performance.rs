//! Opt-in CPU/serialization benchmark. Never renders pages or creates windows.

use std::{sync::Arc, time::Instant};

use super::*;

#[test]
#[ignore = "manual save benchmark; no rendering or windows"]
fn benchmark_dense_save() {
    let base = fixture(90);
    let template = stroke_for(&base);
    let strokes: Vec<_> = (0..1000)
        .map(|i| {
            let mut stroke = template.clone();
            stroke.id = Uuid::new_v4();
            stroke.style.color_rgb[0] = (i % 256) as u8;
            // Dense 180 Hz-style sampling, with a curved centerline and pressure.
            stroke.modeled_samples = (0..400)
                .map(|j| {
                    let t = j as f64 / 399.;
                    crate::document::ModeledSample {
                        position: Point::new(
                            40. + t * 180.,
                            100. + (t * 6. + i as f64).sin() * 15.,
                        ),
                        pressure: 0.5 + (t * 9.).sin() * 0.4,
                        time_us: j * 5556,
                    }
                })
                .collect();
            stroke.outline = crate::ink::saved_outline_v1(&stroke.modeled_samples, &stroke.style);
            Arc::new(stroke)
        })
        .collect();
    let started = Instant::now();
    let (bytes, data) = serialize(&base, &strokes, Uuid::new_v4()).unwrap();
    let serialize_time = started.elapsed();
    let directory = TestDirectory::new();
    let output = directory.file("save-benchmark.pdf");
    let started = Instant::now();
    crate::storage::save_atomic(&output, None, &bytes, |path| {
        let disk = read_pdf(path)?;
        ensure!(disk == bytes, "disk bytes changed");
        Ok(())
    })
    .unwrap();
    assert_eq!(data.strokes.len(), strokes.len());
    println!(
        "1000 strokes / 400000 samples: serialize+validate {serialize_time:?}, disk+validate {:?}, {} bytes",
        started.elapsed(),
        bytes.len()
    );
}
