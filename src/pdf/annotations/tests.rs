//! Dictionary, vector-command and history checks; no pixels or windows.

use super::*;
use crate::document::{EditHistory, ReferenceLayer, ReferenceLines};

fn fixture(rotation: i32) -> (Vec<u8>, Uuid, Uuid) {
    let mut document = PdfDocument::new();
    let mut page = document
        .new_page_at(0, mupdf::Size::new(600., 800.))
        .unwrap();
    page.set_rotation(rotation).unwrap();
    page.object()
        .dict_put(
            "CropBox",
            array(&document, &[20., 30., 550., 760.]).unwrap(),
        )
        .unwrap();
    page.object()
        .dict_put("UserUnit", PdfObject::new_real(1.5).unwrap())
        .unwrap();
    document
        .new_page_at(1, mupdf::Size::new(700., 900.))
        .unwrap();
    let geometry = page_geometry(&page).unwrap();
    let first = Arc::new(crate::ink::test_support::pressure_stroke(geometry).unwrap());
    let mut second = crate::ink::test_support::pressure_stroke(geometry).unwrap();
    second.style.kind = crate::document::InkKind::Highlighter;
    let second = Arc::new(second);
    let mut base = Vec::new();
    document.write_to(&mut base).unwrap();
    let (bytes, _) = super::super::save::serialize_with_reference(
        &base,
        &[first.clone(), second.clone()],
        Uuid::new_v4(),
        layer(),
        &[],
    )
    .unwrap();
    (bytes, first.id, second.id)
}

fn layer() -> ReferenceLayer {
    ReferenceLayer {
        lines: ReferenceLines::default(),
        embed_in_pdf: false,
        auto_expand: false,
    }
}

fn object_by_name(document: &PdfDocument, name: Uuid) -> PdfObject {
    (0..document.page_count().unwrap())
        .flat_map(|index| {
            document
                .load_pdf_page(index)
                .unwrap()
                .annotations()
                .map(|a| a.object())
                .collect::<Vec<_>>()
        })
        .find(|object| {
            object
                .get_dict("NM")
                .unwrap()
                .and_then(|id| id.as_string().ok())
                .as_deref()
                == Some(name.to_string().as_str())
        })
        .unwrap()
}

fn write(document: &PdfDocument) -> Vec<u8> {
    let mut bytes = Vec::new();
    document.write_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn failed_verification_marks_current_annotation_without_restricting_delete_move_rotate_or_undo() {
    for rotation in [0, 90, 180, 270] {
        let (bytes, changed, untouched) = fixture(rotation);
        let document = PdfDocument::from_bytes(&bytes).unwrap();
        let mut object = object_by_name(&document, changed);
        // External movement changes Rect and keeps its current pressure AP.
        let old = object.get_dict("Rect").unwrap().unwrap();
        let old = old
            .array_iter()
            .unwrap()
            .map(|n| n.unwrap().as_float().unwrap() as f64)
            .collect::<Vec<_>>();
        object
            .dict_put(
                "Rect",
                array(
                    &document,
                    &[old[0] + 17., old[1] + 9., old[2] + 17., old[3] + 9.],
                )
                .unwrap(),
            )
            .unwrap();
        let current_ap = object
            .get_dict("AP")
            .unwrap()
            .unwrap()
            .get_dict("N")
            .unwrap()
            .unwrap()
            .read_stream()
            .unwrap();
        let external = write(&document);
        let document = PdfDocument::from_bytes(&external).unwrap();
        let (data, _) = super::super::save::load_edit_data(&document);
        assert_eq!(data.as_ref().unwrap().strokes.len(), 1);
        let (objects, sources) = load_objects(&document, data.as_ref()).unwrap();
        let imported = objects
            .iter()
            .find(|object| !object.edit_data_verified)
            .unwrap();
        let source = imported.pdf_annotation.as_ref().unwrap();
        assert_eq!(sources.len(), 1);
        assert!(!source.svg.is_empty());
        assert!(
            objects
                .iter()
                .any(|object| object.id == untouched && object.edit_data_verified)
        );
        assert!(
            document
                .load_pdf_page(0)
                .unwrap()
                .annotations()
                .next()
                .is_none()
        );
        assert!(imported.hit_segment(imported.bounds().center(), imported.bounds().center(), 0.));
        let ids = [imported.id].into_iter().collect();
        let mut history = EditHistory::from_strokes(objects.iter().cloned());
        let transform = InkTransform {
            center: imported.bounds().center(),
            translation: crate::document::Point::new(21., -13.),
            radians: 0.47,
        };
        assert!(history.transform_strokes(&ids, transform));
        assert!(
            !history
                .strokes(0)
                .iter()
                .find(|s| s.id == imported.id)
                .unwrap()
                .edit_data_verified
        );
        history.undo();
        assert!(Arc::ptr_eq(
            history
                .strokes(0)
                .iter()
                .find(|s| s.id == imported.id)
                .unwrap(),
            imported
        ));
        history.redo();
        assert!(history.move_strokes(&ids, InkTransform::default(), 1));
        let moved = history.strokes(1)[0].clone();
        let (saved, saved_data) = super::super::save::serialize_with_annotations(
            &external,
            &history.snapshot(),
            Uuid::new_v4(),
            layer(),
            &[],
            &sources,
        )
        .unwrap();
        // Unverified objects retain their standard dictionary, never fake capture data.
        assert_eq!(saved_data.strokes.len(), 1);
        let reopened = PdfDocument::from_bytes(&saved).unwrap();
        let moved_object = object_by_name(&reopened, changed);
        assert!(
            reopened
                .load_pdf_page(0)
                .unwrap()
                .annotations()
                .all(|a| a.xref().unwrap() != source.xref)
        );
        assert!(
            reopened
                .load_pdf_page(1)
                .unwrap()
                .annotations()
                .any(|a| a.xref().unwrap() == source.xref)
        );
        let wrapper = moved_object
            .get_dict("AP")
            .unwrap()
            .unwrap()
            .get_dict("N")
            .unwrap()
            .unwrap();
        let original = wrapper
            .get_dict("Resources")
            .unwrap()
            .unwrap()
            .get_dict("XObject")
            .unwrap()
            .unwrap()
            .get_dict("NNOriginal")
            .unwrap()
            .unwrap();
        assert_eq!(original.read_stream().unwrap(), current_ap);
        let (data, _) = super::super::save::load_edit_data(&reopened);
        let (again, sources_again) = load_objects(&reopened, data.as_ref()).unwrap();
        let restored = again.iter().find(|s| s.pdf_annotation.is_some()).unwrap();
        assert!(!restored.edit_data_verified);
        for (actual, expected) in [restored.bounds().min, restored.bounds().max]
            .into_iter()
            .zip([moved.bounds().min, moved.bounds().max])
        {
            assert!(
                (actual.x - expected.x).abs() < 0.01 && (actual.y - expected.y).abs() < 0.01,
                "{actual:?} != {expected:?}"
            );
        }
        // Delete and undo work on the same unverified object and preserve its AP.
        assert!(history.remove_strokes(&ids));
        let (deleted, _) = super::super::save::serialize_with_annotations(
            &external,
            &history.snapshot(),
            Uuid::new_v4(),
            layer(),
            &[],
            &sources,
        )
        .unwrap();
        let deleted = PdfDocument::from_bytes(&deleted).unwrap();
        assert_eq!(deleted.load_pdf_page(0).unwrap().annotations().count(), 1);
        assert_eq!(deleted.load_pdf_page(1).unwrap().annotations().count(), 0);
        history.undo();
        assert!(Arc::ptr_eq(&history.strokes(1)[0], &moved));
        let (again_saved, _) = super::super::save::serialize_with_annotations(
            &saved,
            &again,
            Uuid::new_v4(),
            layer(),
            &[],
            &sources_again,
        )
        .unwrap();
        let again_saved = PdfDocument::from_bytes(&again_saved).unwrap();
        assert_eq!(
            again_saved.load_pdf_page(1).unwrap().annotations().count(),
            1
        );
    }
}

#[test]
fn no_private_attachment_still_allows_standard_objects_and_keeps_highlighter_blending() {
    let (bytes, _, _) = fixture(0);
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    document.catalog().unwrap().dict_delete("Names").unwrap();
    let bytes = write(&document);
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let (objects, sources) = load_objects(&document, None).unwrap();
    assert_eq!(objects.len(), 2);
    assert!(objects.iter().all(|object| !object.edit_data_verified));
    assert!(sources.iter().any(|source| source.multiply));
    let (saved, data) = super::super::save::serialize_with_annotations(
        &bytes,
        &[],
        Uuid::new_v4(),
        layer(),
        &[],
        &sources,
    )
    .unwrap();
    assert!(data.strokes.is_empty());
    assert_eq!(
        PdfDocument::from_bytes(&saved)
            .unwrap()
            .load_pdf_page(0)
            .unwrap()
            .annotations()
            .count(),
        0
    );
}

#[test]
fn standard_text_without_private_data_keeps_contents_and_popup_during_move_rotation_and_delete() {
    let mut document = PdfDocument::new();
    let mut page = document
        .new_page_at(0, mupdf::Size::new(600., 800.))
        .unwrap();
    let mut note = page
        .add_text_annotation(
            mupdf::Rect::new(70., 80., 100., 110.),
            "externally edited contents",
        )
        .unwrap();
    note.set_popup(mupdf::Rect::new(110., 80., 250., 190.))
        .unwrap();
    note.update().unwrap();
    document
        .new_page_at(1, mupdf::Size::new(600., 800.))
        .unwrap();
    let bytes = write(&document);
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let (objects, sources) = load_objects(&document, None).unwrap();
    assert_eq!(objects.len(), 1);
    let mut moved = objects[0].transformed(InkTransform {
        center: objects[0].bounds().center(),
        translation: crate::document::Point::new(25., 30.),
        radians: 0.5,
    });
    moved.page_index = 1;
    assert!(!moved.edit_data_verified);
    let (saved, data) = super::super::save::serialize_with_annotations(
        &bytes,
        &[Arc::new(moved)],
        Uuid::new_v4(),
        layer(),
        &[],
        &sources,
    )
    .unwrap();
    assert!(data.strokes.is_empty());
    let saved = PdfDocument::from_bytes(&saved).unwrap();
    let note = saved
        .load_pdf_page(1)
        .unwrap()
        .annotations()
        .next()
        .unwrap()
        .object();
    assert_eq!(
        note.get_dict("Contents")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        "externally edited contents"
    );
    assert_eq!(
        note.get_dict("Subtype")
            .unwrap()
            .unwrap()
            .as_name()
            .unwrap(),
        b"Text"
    );
    let popup = note.get_dict("Popup").unwrap().unwrap();
    assert_eq!(
        saved
            .lookup_page_number(&popup.get_dict("P").unwrap().unwrap())
            .unwrap(),
        1
    );
    let (deleted, _) = super::super::save::serialize_with_annotations(
        &bytes,
        &[],
        Uuid::new_v4(),
        layer(),
        &[],
        &sources,
    )
    .unwrap();
    let deleted = PdfDocument::from_bytes(&deleted).unwrap();
    assert_eq!(
        deleted
            .find_page(0)
            .unwrap()
            .get_dict("Annots")
            .unwrap()
            .unwrap()
            .len()
            .unwrap(),
        0
    );
}

#[test]
fn unavailable_private_settings_do_not_strip_existing_reference_vectors_on_save() {
    let (bytes, _, _) = fixture(0);
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let (data, _) = super::super::save::load_edit_data(&document);
    let mut embedded = layer();
    embedded.embed_in_pdf = true;
    let snapshot = data
        .unwrap()
        .strokes
        .into_iter()
        .map(|saved| Arc::new(saved.stroke))
        .collect::<Vec<_>>();
    let (bytes, _) = super::super::save::serialize_with_reference(
        &bytes,
        &snapshot,
        Uuid::new_v4(),
        embedded,
        &[],
    )
    .unwrap();
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    document.catalog().unwrap().dict_delete("Names").unwrap();
    let bytes = write(&document);
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let (_, sources) = load_objects(&document, None).unwrap();
    let contents = |document: &PdfDocument| {
        document
            .find_page(0)
            .unwrap()
            .get_dict("Contents")
            .unwrap()
            .unwrap()
            .array_iter()
            .unwrap()
            .map(|stream| stream.unwrap().read_stream().unwrap())
            .collect::<Vec<_>>()
    };
    let original = contents(&document);
    let (saved, data) = super::super::save::serialize_with_annotations(
        &bytes,
        &[],
        Uuid::new_v4(),
        ReferenceLayer {
            lines: ReferenceLines {
                pattern: crate::document::ReferencePattern::None,
                ..Default::default()
            },
            embed_in_pdf: false,
            auto_expand: false,
        },
        &[],
        &sources,
    )
    .unwrap();
    assert!(data.reference_lines.is_none());
    assert_eq!(
        contents(&PdfDocument::from_bytes(&saved).unwrap()),
        original
    );
}
