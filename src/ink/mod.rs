//! Direct pressure strokes and a deterministic PDF appearance fixture.

mod direct;
mod outline;
mod target;

pub use direct::DirectStroke;
use outline::round_outline;
pub use target::StrokeTarget;

use anyhow::{Result, ensure};
use uuid::Uuid;

use crate::document::{
    ModeledSample, PageGeometry, PenPhase, PenStyle, Point, ProcessingSnapshot, RawSample,
    StrokeRecord,
};

pub fn demo_stroke(page: PageGeometry) -> Result<StrokeRecord> {
    let size = page.paper_size();
    ensure!(
        size.width >= 40.0 && size.height >= 40.0,
        "Page is too small for the pressure demo"
    );
    let style = PenStyle {
        color_rgb: [25, 105, 190],
        max_width_pt: 8.0_f64.min(size.height * 0.06),
        min_width_ratio: 0.15,
        pressure_gamma: 1.5,
    };
    let raw_samples: Vec<_> = (0..=96)
        .map(|i| {
            let t = f64::from(i) / 96.0;
            RawSample {
                position: Point::new(
                    size.width * (0.15 + t * 0.70),
                    size.height * (0.42 + 0.055 * (t * std::f64::consts::TAU).sin()),
                ),
                time_us: i as u64 * 8_000,
                pressure: Some(0.04 + 0.96 * (t * std::f64::consts::PI).sin().powi(2)),
                tilt_degrees: Some([0.0, 0.0]),
                rotation_degrees: Some(0.0),
                native: None,
                phase: match i {
                    0 => PenPhase::Down,
                    96 => PenPhase::Up,
                    _ => PenPhase::Move,
                },
            }
        })
        .collect();
    let modeled_samples: Vec<_> = raw_samples
        .iter()
        .map(|sample| ModeledSample {
            position: sample.position,
            time_us: sample.time_us,
            pressure: sample.pressure.unwrap_or(0.5),
        })
        .collect();
    let outline = round_outline(&modeled_samples, &style);
    Ok(StrokeRecord {
        id: Uuid::new_v4(),
        page_index: 0,
        raw_samples,
        modeled_samples,
        outline,
        style,
        processing: ProcessingSnapshot {
            algorithm: "simulated-direct-round-outline".to_owned(),
            version: 1,
            coordinate_unit: "paper-pt-top-left".to_owned(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{PathCommand, Rect, Rotation};

    #[test]
    fn pressure_fixture_is_bounded_and_wider_in_the_middle() {
        for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            let page =
                PageGeometry::new(Rect::new(-20.0, 30.0, 580.0, 830.0), rotation, 1.5).unwrap();
            let stroke = demo_stroke(page).unwrap();
            assert_eq!(stroke.raw_samples.len(), 97);
            assert!(
                stroke
                    .modeled_samples
                    .iter()
                    .all(|s| page.contains_paper(s.position))
            );
            assert!(
                stroke.style.width(stroke.modeled_samples[48].pressure)
                    > 5.0 * stroke.style.width(stroke.modeled_samples[0].pressure)
            );
            assert_eq!(stroke.outline.last(), Some(&PathCommand::Close));
        }
    }

    #[test]
    fn coincident_samples_and_single_point_have_finite_round_geometry() {
        let sample = ModeledSample {
            position: Point::new(5.0, 6.0),
            time_us: 0,
            pressure: 0.5,
        };
        let style = PenStyle {
            color_rgb: [0; 3],
            max_width_pt: 2.0,
            min_width_ratio: 0.2,
            pressure_gamma: 1.5,
        };
        for samples in [vec![sample.clone()], vec![sample.clone(), sample]] {
            let path = round_outline(&samples, &style);
            assert_eq!(path.len(), samples.len() * 6);
            for command in path {
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
    }
}
