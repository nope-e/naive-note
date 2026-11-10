//! Structural external-edit round trips; no rasterization or window operations.

use std::sync::Arc;

use super::*;

fn saved_ink(rotation: i32) -> (Vec<u8>, EditData) {
    let base = fixture(rotation);
    let document = PdfDocument::from_bytes(&base).unwrap();
    let first = Arc::new(stroke_for(&base));
    let mut second = stroke_for(&base);
    second.style.kind = InkKind::Highlighter;
    let mut third =
        pressure_stroke(page_geometry(&document.load_pdf_page(1).unwrap()).unwrap()).unwrap();
    third.page_index = 1;
    serialize_with_reference(
        &base,
        &[first, Arc::new(second), Arc::new(third)],
        Uuid::new_v4(),
        ReferenceLayer {
            lines: crate::document::ReferenceLines {
                pattern: crate::document::ReferencePattern::Dots,
                color_rgb: [120, 150, 180],
                ..Default::default()
            },
            embed_in_pdf: true,
            auto_expand: false,
        },
        &[],
    )
    .unwrap()
}

fn annotation(document: &PdfDocument, saved: &SavedStroke) -> PdfObject {
    document
        .load_pdf_page(saved.stroke.page_index as i32)
        .unwrap()
        .annotations()
        .find(|a| {
            required(&a.object(), "NM")
                .and_then(|name| Ok(name.as_string()?))
                .is_ok_and(|name| name == saved.stroke.id.to_string())
        })
        .unwrap()
        .object()
}

fn delete_ink(document: &PdfDocument, saved: &SavedStroke) {
    let mut page = document
        .load_pdf_page(saved.stroke.page_index as i32)
        .unwrap();
    let annotation = page
        .annotations()
        .find(|a| {
            required(&a.object(), "NM")
                .and_then(|name| Ok(name.as_string()?))
                .is_ok_and(|name| name == saved.stroke.id.to_string())
        })
        .unwrap();
    page.delete_annotation(annotation).unwrap();
}

fn write(document: &PdfDocument) -> Vec<u8> {
    let mut bytes = Vec::new();
    document.write_to(&mut bytes).unwrap();
    bytes
}

fn snapshot(data: &EditData) -> Vec<Arc<StrokeRecord>> {
    data.strokes
        .iter()
        .map(|saved| Arc::new(saved.stroke.clone()))
        .collect()
}

fn save_snapshot(
    base: &[u8],
    strokes: &[Arc<StrokeRecord>],
    data: &EditData,
) -> (Vec<u8>, EditData) {
    serialize_with_reference(
        base,
        strokes,
        Uuid::new_v4(),
        data.reference_lines.unwrap(),
        &[],
    )
    .unwrap()
}

#[test]
fn deleted_ink_stays_deleted_while_other_records_and_document_settings_survive_resaving() {
    for rotation in [0, 90, 180, 270] {
        for deleted in [vec![0], vec![1, 2], vec![0, 1, 2]] {
            let (bytes, original) = saved_ink(rotation);
            let document = PdfDocument::from_bytes(&bytes).unwrap();
            for &index in &deleted {
                delete_ink(&document, &original.strokes[index]);
            }
            let external = write(&document);
            // Our own save verification must never silently accept lost ink.
            assert!(verify_data(&external, &original).is_err());
            let reopened = PdfDocument::from_bytes(&external).unwrap();
            let (loaded, status) = load_edit_data(&reopened);
            let loaded = loaded.unwrap();
            let mut expected = original.clone();
            expected.strokes = original
                .strokes
                .iter()
                .enumerate()
                .filter(|(index, _)| !deleted.contains(index))
                .map(|(_, saved)| saved.clone())
                .collect();
            assert_eq!(loaded, expected);
            assert!(status.contains(&format!(
                "{} deleted or changed stroke(s) skipped",
                deleted.len()
            )));

            let fresh = Arc::new(stroke_for(&external));
            let mut records = snapshot(&loaded);
            records.push(fresh.clone());
            let (saved, data) = save_snapshot(&external, &records, &loaded);
            assert_eq!(data.document_id, original.document_id);
            assert_eq!(data.reference_lines, original.reference_lines);
            assert_eq!(data.strokes.len(), expected.strokes.len() + 1);
            verify_data(&saved, &data).unwrap();
            for &index in &deleted {
                assert!(
                    data.strokes
                        .iter()
                        .all(|s| s.stroke.id != original.strokes[index].stroke.id)
                );
            }
            // Repeated saving cannot restore deleted objects or duplicate survivors.
            let (again, _) = save_snapshot(&saved, &records, &data);
            let reopened = PdfDocument::from_bytes(&again).unwrap();
            assert_eq!(read_edit_data(&reopened).unwrap().unwrap(), data);
            let count: usize = (0..reopened.page_count().unwrap())
                .map(|page| reopened.load_pdf_page(page).unwrap().annotations().count())
                .sum();
            assert_eq!(count, records.len() + 1); // One original third-party text annotation.
            assert_eq!(
                reopened
                    .load_embedded_file("existing.txt")
                    .unwrap()
                    .unwrap(),
                b"keep this attachment"
            );
            // The remaining native objects can still be erased after reopening.
            let (_, erased) = save_snapshot(&again, &[fresh], &data);
            assert_eq!(erased.strokes.len(), 1);
        }
    }
}

#[test]
fn externally_changed_ink_is_preserved_while_untouched_ink_remains_editable() {
    for key in ["NM", "InkList", "Rect", "F", "AP", "Subtype", "Resources"] {
        let (bytes, original) = saved_ink(90);
        let document = PdfDocument::from_bytes(&bytes).unwrap();
        let changed = &original.strokes[1];
        let mut object = annotation(&document, changed);
        match key {
            "NM" => object
                .dict_put(key, PdfObject::new_string("external-id").unwrap())
                .unwrap(),
            "F" => object
                .dict_put(key, PdfObject::new_int(0).unwrap())
                .unwrap(),
            "Subtype" => object
                .dict_put(key, PdfObject::new_name("Stamp").unwrap())
                .unwrap(),
            "InkList" => required(&object, key)
                .unwrap()
                .get_array(0)
                .unwrap()
                .unwrap()
                .array_put(0, PdfObject::new_real(999.).unwrap())
                .unwrap(),
            "Rect" => required(&object, key)
                .unwrap()
                .array_put(0, PdfObject::new_real(999.).unwrap())
                .unwrap(),
            "AP" => normal_appearance(&object)
                .unwrap()
                .write_stream_string("q 1 0 0 rg 0 0 10 10 re f Q")
                .unwrap(),
            "Resources" => required(
                &required(
                    &required(&normal_appearance(&object).unwrap(), key).unwrap(),
                    "ExtGState",
                )
                .unwrap(),
                "NNHighlight",
            )
            .unwrap()
            .dict_put("ca", PdfObject::new_real(1.0).unwrap())
            .unwrap(),
            _ => unreachable!(),
        }
        let external = write(&document);
        let document = PdfDocument::from_bytes(&external).unwrap();
        let (loaded, status) = load_edit_data(&document);
        let loaded = loaded.unwrap();
        assert_eq!(
            loaded.strokes,
            [original.strokes[0].clone(), original.strokes[2].clone()],
            "{key}"
        );
        assert_eq!(loaded.reference_lines, original.reference_lines);
        assert!(status.contains("1 deleted or changed stroke(s) skipped"));
        assert!(verify_data(&external, &original).is_err());
        // Removing all verified ink must leave the externally changed object intact.
        let (saved, data) = save_snapshot(&external, &[], &loaded);
        assert!(data.strokes.is_empty());
        let reopened = PdfDocument::from_bytes(&saved).unwrap();
        assert_eq!(reopened.load_pdf_page(0).unwrap().annotations().count(), 2);
        assert_eq!(reopened.load_pdf_page(1).unwrap().annotations().count(), 0);
        let name = if key == "NM" {
            "external-id".to_owned()
        } else {
            changed.stroke.id.to_string()
        };
        let preserved = reopened
            .load_pdf_page(0)
            .unwrap()
            .annotations()
            .find(|a| {
                required(&a.object(), "NM")
                    .and_then(|n| Ok(n.as_string()?))
                    .is_ok_and(|n| n == name)
            })
            .unwrap()
            .object();
        // Compare the PDF objects and appearance streams, without inspecting pixels.
        assert_eq!(
            fingerprint(&preserved).unwrap(),
            fingerprint(&object).unwrap()
        );
        assert_eq!(
            normal_appearance(&preserved)
                .unwrap()
                .read_stream()
                .unwrap(),
            normal_appearance(&object).unwrap().read_stream().unwrap()
        );
    }
}

#[test]
fn duplicated_annotation_ids_on_any_page_do_not_remove_or_restore_ambiguous_ink() {
    for duplicate_page in [0, 1] {
        let (bytes, original) = saved_ink(0);
        let document = PdfDocument::from_bytes(&bytes).unwrap();
        let id = original.strokes[0].stroke.id.to_string();
        let mut page = document.load_pdf_page(duplicate_page).unwrap();
        let mut duplicate = page
            .add_text_annotation(mupdf::Rect::new(40., 40., 60., 60.), "Duplicate identifier")
            .unwrap();
        duplicate.update().unwrap();
        duplicate
            .object()
            .dict_put("NM", PdfObject::new_string(&id).unwrap())
            .unwrap();
        let external = write(&document);
        let reopened = PdfDocument::from_bytes(&external).unwrap();
        let (loaded, status) = load_edit_data(&reopened);
        let loaded = loaded.unwrap();
        assert_eq!(loaded.strokes, original.strokes[1..]);
        assert!(status.contains("1 deleted or changed stroke(s) skipped"));
        assert!(verify_data(&external, &original).is_err());
        let (saved, data) = save_snapshot(&external, &snapshot(&loaded), &loaded);
        verify_data(&saved, &data).unwrap();
        let document = PdfDocument::from_bytes(&saved).unwrap();
        let matching = (0..document.page_count().unwrap())
            .flat_map(|index| {
                document
                    .load_pdf_page(index)
                    .unwrap()
                    .annotations()
                    .map(|a| a.object())
                    .collect::<Vec<_>>()
            })
            .filter(|object| {
                required(object, "NM")
                    .and_then(|n| Ok(n.as_string()?))
                    .is_ok_and(|n| n == id)
            })
            .count();
        assert_eq!(matching, 2);
    }
}
