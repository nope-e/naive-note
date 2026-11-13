//! Size and compatibility regressions: PDF objects only, no rendering or windows.

use std::sync::Arc;

use super::*;
use crate::document::{EditHistory, InkTransform};

/// Reproduce the old writer: full capture data, uncompressed streams, no GC.
pub(super) fn legacy_serialize(
    base: &[u8],
    strokes: &[Arc<StrokeRecord>],
    document_id: Uuid,
) -> (Vec<u8>, EditData) {
    previous_serialize(base, strokes, document_id, 1)
}

fn previous_serialize(
    base: &[u8],
    strokes: &[Arc<StrokeRecord>],
    document_id: Uuid,
    version: u32,
) -> (Vec<u8>, EditData) {
    let mut document = PdfDocument::from_bytes(base).unwrap();
    let data = EditData {
        format_version: version,
        document_id,
        strokes: strokes
            .iter()
            .map(|stroke| {
                let mut saved = add_stroke_version(&mut document, stroke, 1).unwrap();
                if version == 1 {
                    saved.stroke = (**stroke).clone();
                }
                // Earlier writers added the same visible comment to every
                // native stroke. Keep it in fixtures to test its migration.
                let page = document.load_pdf_page(stroke.page_index as i32).unwrap();
                let mut object = page.annotations().last().unwrap().object();
                object
                    .dict_put(
                        "Contents",
                        PdfObject::new_string("Naive Note pressure stroke").unwrap(),
                    )
                    .unwrap();
                saved
            })
            .collect(),
        reference_lines: Some(ReferenceLayer {
            lines: crate::document::ReferenceLines::default(),
            embed_in_pdf: false,
            auto_expand: true,
        }),
    };
    document
        .add_embedded_file(
            &data.filename(),
            &data.encode().unwrap(),
            EmbeddedFileOptions::new(&data.filename()),
        )
        .unwrap();
    let mut bytes = Vec::new();
    let mut options = PdfWriteOptions::default();
    if version >= 2 {
        options.set_compress(true).set_garbage_level(1);
    }
    document.write_to_with_options(&mut bytes, options).unwrap();
    (bytes, data)
}

#[test]
fn thousand_strokes_shrink_without_changing_confirmed_geometry_or_live_capture() {
    let base = fixture(90);
    let template = stroke_for(&base);
    let strokes: Vec<_> = (0..1000)
        .map(|index| {
            let mut stroke = template.clone();
            stroke.id = Uuid::new_v4();
            stroke.style.color_rgb[0] = (index % 256) as u8;
            if index % 3 == 0 {
                stroke.style.kind = InkKind::Highlighter;
            }
            // Distinct trajectories and pressure avoid hiding duplicated
            // outlines behind cross-stroke compression of identical fixtures.
            for (i, sample) in stroke.modeled_samples.iter_mut().enumerate() {
                let phase = f64::from(index) * 0.31 + i as f64 * 0.04;
                sample.position.y += phase.sin() * 8.;
                sample.position.x += (phase * 0.63).cos() * 3.;
                sample.pressure = (0.5 + (phase * 1.17).sin() * 0.45).clamp(0., 1.);
            }
            stroke.outline = crate::ink::saved_outline_v1(&stroke.modeled_samples, &stroke.style);
            if index % 20 == 0 {
                stroke = stroke.transformed(InkTransform {
                    translation: Point::new(12., -7.),
                    radians: 0.42,
                    ..Default::default()
                });
            }
            Arc::new(stroke)
        })
        .collect();
    let id = Uuid::new_v4();
    let (legacy, _) = legacy_serialize(&base, &strokes, id);
    let (previous, previous_data) = previous_serialize(&base, &strokes, id, 2);
    assert_eq!(
        read_edit_data(&PdfDocument::from_bytes(&previous).unwrap())
            .unwrap()
            .unwrap(),
        previous_data
    );
    let (bytes, data) = serialize(&base, &strokes, id).unwrap();
    println!(
        "1000 strokes: original {} bytes, v2 {} bytes, current {} bytes ({:.1}% smaller than v2)",
        legacy.len(),
        previous.len(),
        bytes.len(),
        100. * (1. - bytes.len() as f64 / previous.len() as f64)
    );
    assert!(bytes.len() < legacy.len() / 2);
    assert!(bytes.len() < previous.len());
    assert_eq!(data.format_version, FORMAT_VERSION);
    for (saved, original) in data.strokes.iter().zip(&strokes) {
        assert_eq!(saved.stroke, without_capture(original));
        assert_eq!(original.raw_samples, template.raw_samples);
    }
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    for annotation in document.load_pdf_page(0).unwrap().annotations() {
        if is_name(annotation.object().get_dict("Subtype").unwrap(), b"Ink").unwrap() {
            assert!(annotation.object().get_dict("Contents").unwrap().is_none());
            let ap = normal_appearance(&annotation.object()).unwrap();
            assert!(is_name(ap.get_dict("Filter").unwrap(), b"FlateDecode").unwrap());
        }
    }
    assert_eq!(read_edit_data(&document).unwrap().unwrap(), data);
}

#[test]
fn legacy_capture_is_removed_and_reopened_objects_still_edit_and_undo() {
    for rotation in [0, 90, 180, 270] {
        let base = fixture(rotation);
        let stroke = Arc::new(stroke_for(&base));
        let id = Uuid::new_v4();
        let (legacy, old) = legacy_serialize(&base, std::slice::from_ref(&stroke), id);
        let document = PdfDocument::from_bytes(&legacy).unwrap();
        assert_eq!(read_edit_data(&document).unwrap().unwrap(), old);
        let (first, saved) = serialize(&legacy, std::slice::from_ref(&stroke), id).unwrap();
        assert_eq!(saved.strokes[0].stroke, without_capture(&stroke));
        let document = PdfDocument::from_bytes(&first).unwrap();
        let native = document
            .load_pdf_page(0)
            .unwrap()
            .annotations()
            .last()
            .unwrap()
            .object();
        assert!(native.get_dict("Contents").unwrap().is_none());
        let old_attachment = document
            .load_embedded_file(&saved.filename())
            .unwrap()
            .unwrap();
        let cbor = zstd::stream::decode_all(old_attachment.as_slice()).unwrap();
        let value: ciborium::Value = ciborium::from_reader(cbor.as_slice()).unwrap();
        // Empty capture is omitted, rather than written alongside the geometry.
        assert!(!format!("{value:?}").contains("raw_samples"));
        assert!(format!("{value:?}").contains("samples-v1"));
        let mut history =
            EditHistory::from_strokes(saved.strokes.iter().map(|s| Arc::new(s.stroke.clone())));
        let ids = [stroke.id].into_iter().collect();
        assert!(history.move_strokes(
            &ids,
            InkTransform {
                center: stroke.bounds().center(),
                translation: Point::new(12., -7.),
                radians: 0.42,
            },
            1,
        ));
        let (moved, _) = serialize(&first, &history.snapshot(), id).unwrap();
        let document = PdfDocument::from_bytes(&moved).unwrap();
        assert_eq!(
            read_edit_data(&document).unwrap().unwrap().strokes[0].stroke,
            *history.snapshot()[0]
        );
        assert_eq!(document.load_pdf_page(0).unwrap().annotations().count(), 1);
        assert_eq!(document.load_pdf_page(1).unwrap().annotations().count(), 1);
        assert!(history.undo());
        let (_, restored) = serialize(&moved, &history.snapshot(), id).unwrap();
        assert_eq!(restored.strokes[0].stroke, saved.strokes[0].stroke);
        history.remove_strokes(&ids);
        let (erased, empty) = serialize(&moved, &history.snapshot(), id).unwrap();
        assert!(empty.strokes.is_empty());
        let document = PdfDocument::from_bytes(&erased).unwrap();
        assert_eq!(document.load_pdf_page(0).unwrap().annotations().count(), 1);
        assert_eq!(document.load_pdf_page(1).unwrap().annotations().count(), 0);
    }
}

#[test]
fn repeated_saves_collect_replaced_appearances_and_attachments() {
    let base = fixture(0);
    let strokes: Vec<_> = (0..12).map(|_| Arc::new(stroke_for(&base))).collect();
    let id = Uuid::new_v4();
    let (mut bytes, _) = serialize(&base, &strokes, id).unwrap();
    let initial_size = bytes.len();
    for _ in 0..4 {
        let (saved, _) = serialize(&bytes, &strokes, id).unwrap();
        assert!(saved.len() < initial_size + 8192);
        bytes = saved;
    }
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let mut attachments = 0;
    let mut appearances = 0;
    for index in 1..document.count_objects().unwrap() {
        if let Some(object) = document.xref_object(index as i32).unwrap() {
            if is_name(object.get_dict("Type").unwrap(), b"EmbeddedFile").unwrap() {
                attachments += 1;
            }
            if is_name(object.get_dict("Subtype").unwrap(), b"Form").unwrap() {
                appearances += 1;
            }
        }
    }
    assert_eq!(attachments, 2); // Existing text attachment plus current edit data.
    assert_eq!(appearances, strokes.len() + 1); // Existing text annotation AP.
}
