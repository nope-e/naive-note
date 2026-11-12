//! Omit derived outlines only when replay reproduces every coordinate exactly.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeSeq};

use crate::document::{
    InkTransform, ModeledSample, PathCommand, PenStyle, ProcessingSnapshot, ReferenceLayer,
    StrokeRecord,
};

use super::{EditData, SavedStroke};

#[derive(Serialize)]
pub(super) struct EditDataRef<'a> {
    format_version: u32,
    document_id: uuid::Uuid,
    #[serde(serialize_with = "serialize_strokes")]
    strokes: &'a [SavedStroke],
    reference_lines: Option<ReferenceLayer>,
}

impl<'a> EditDataRef<'a> {
    pub fn new(data: &'a EditData) -> Self {
        Self {
            format_version: data.format_version,
            document_id: data.document_id,
            strokes: &data.strokes,
            reference_lines: data.reference_lines,
        }
    }
}

#[derive(Serialize)]
struct SavedStrokeRef<'a> {
    stroke: StrokeRef<'a>,
    annotation_sha256: &'a str,
}

#[derive(Serialize)]
struct StrokeRef<'a> {
    id: uuid::Uuid,
    page_index: u32,
    #[serde(skip_serializing_if = "InkTransform::is_identity")]
    edit_transform: InkTransform,
    modeled_samples: &'a [ModeledSample],
    #[serde(skip_serializing_if = "Option::is_none")]
    outline: Option<&'a [PathCommand]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outline_encoding: Option<&'static str>,
    style: &'a PenStyle,
    processing: &'a ProcessingSnapshot,
}

fn serialize_strokes<S: Serializer>(
    strokes: &[SavedStroke],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut sequence = serializer.serialize_seq(Some(strokes.len()))?;
    for saved in strokes {
        let stroke = &saved.stroke;
        // Rigid edits and legacy algorithms may produce different control
        // points. Such records keep their exact explicit outlines.
        let derived = same_outline(
            &crate::ink::saved_outline_v1(&stroke.modeled_samples, &stroke.style),
            &stroke.outline,
        );
        sequence.serialize_element(&SavedStrokeRef {
            stroke: StrokeRef {
                id: stroke.id,
                page_index: stroke.page_index,
                edit_transform: stroke.edit_transform,
                modeled_samples: &stroke.modeled_samples,
                outline: (!derived).then_some(stroke.outline.as_slice()),
                outline_encoding: derived.then_some("samples-v1"),
                style: &stroke.style,
                processing: &stroke.processing,
            },
            annotation_sha256: &saved.annotation_sha256,
        })?;
    }
    sequence.end()
}

fn same_outline(a: &[PathCommand], b: &[PathCommand]) -> bool {
    // Preserve even the sign of zero, which can affect exact PDF AP text.
    let point = |a: crate::document::Point, b: crate::document::Point| {
        a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits()
    };
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| match (a, b) {
            (PathCommand::MoveTo(a), PathCommand::MoveTo(b))
            | (PathCommand::LineTo(a), PathCommand::LineTo(b)) => point(*a, *b),
            (
                PathCommand::CubicTo {
                    control1: a1,
                    control2: a2,
                    to: a3,
                },
                PathCommand::CubicTo {
                    control1: b1,
                    control2: b2,
                    to: b3,
                },
            ) => point(*a1, *b1) && point(*a2, *b2) && point(*a3, *b3),
            (PathCommand::Close, PathCommand::Close) => true,
            _ => false,
        })
}

impl<'de> Deserialize<'de> for SavedStroke {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct StoredStroke {
            #[serde(flatten)]
            stroke: StrokeRecord,
            #[serde(default)]
            outline_encoding: Option<String>,
        }
        #[derive(Deserialize)]
        struct StoredSavedStroke {
            stroke: StoredStroke,
            annotation_sha256: String,
        }
        let saved = StoredSavedStroke::deserialize(deserializer)?;
        let mut stroke = saved.stroke.stroke;
        if let Some(encoding) = saved.stroke.outline_encoding {
            if encoding != "samples-v1" || !stroke.outline.is_empty() {
                return Err(serde::de::Error::custom(
                    "Unsupported or ambiguous outline encoding",
                ));
            }
            stroke.outline = crate::ink::saved_outline_v1(&stroke.modeled_samples, &stroke.style);
        }
        Ok(Self {
            stroke,
            annotation_sha256: saved.annotation_sha256,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Point;

    #[test]
    fn outline_elision_requires_identical_commands_coordinates_and_signed_zero() {
        let a = [PathCommand::MoveTo(Point::new(0., 1.)), PathCommand::Close];
        assert!(same_outline(&a, &a));
        assert!(!same_outline(&a, &a[..1]));
        assert!(!same_outline(
            &a,
            &[PathCommand::LineTo(Point::new(0., 1.)), PathCommand::Close]
        ));
        assert!(!same_outline(
            &a,
            &[PathCommand::MoveTo(Point::new(-0., 1.)), PathCommand::Close]
        ));
        assert!(!same_outline(
            &a,
            &[
                PathCommand::MoveTo(Point::new(0., 1. + f64::EPSILON)),
                PathCommand::Close
            ]
        ));
        let curve = |x| {
            [PathCommand::CubicTo {
                control1: Point::new(x, 1.),
                control2: Point::new(2., 3.),
                to: Point::new(4., 5.),
            }]
        };
        assert!(same_outline(&curve(0.), &curve(0.)));
        assert!(!same_outline(&curve(0.), &curve(f64::EPSILON)));
    }

    #[test]
    fn outline_decoder_rejects_unknown_or_ambiguous_generation_instructions() {
        #[derive(Serialize)]
        struct StoredStroke<'a> {
            #[serde(flatten)]
            stroke: &'a StrokeRecord,
            outline_encoding: &'a str,
        }
        #[derive(Serialize)]
        struct StoredSavedStroke<'a> {
            stroke: StoredStroke<'a>,
            annotation_sha256: &'a str,
        }
        let page = crate::document::PageGeometry::new(
            crate::document::Rect::new(0., 0., 600., 800.),
            crate::document::Rotation::R0,
            1.,
        )
        .unwrap();
        let original = crate::ink::test_support::pressure_stroke(page).unwrap();
        let decode = |stroke: &StrokeRecord, encoding| {
            let mut bytes = Vec::new();
            ciborium::into_writer(
                &StoredSavedStroke {
                    stroke: StoredStroke {
                        stroke,
                        outline_encoding: encoding,
                    },
                    annotation_sha256: "test",
                },
                &mut bytes,
            )
            .unwrap();
            ciborium::from_reader::<SavedStroke, _>(bytes.as_slice())
        };
        assert!(decode(&original, "samples-v1").is_err());
        let mut omitted = original.clone();
        omitted.outline.clear();
        assert!(decode(&omitted, "samples-v999").is_err());
        assert_eq!(decode(&omitted, "samples-v1").unwrap().stroke, original);
    }
}
