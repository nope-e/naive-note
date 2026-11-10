//! Structural PDF/CBOR checks only. No rendering, pixels, windows or viewers.

use std::path::PathBuf;

use super::*;
use crate::ink::test_support::pressure_stroke;

fn serialize(
    base: &[u8],
    strokes: &[std::sync::Arc<StrokeRecord>],
    id: Uuid,
) -> Result<(Vec<u8>, EditData)> {
    serialize_with_reference(
        base,
        strokes,
        id,
        ReferenceLayer {
            lines: crate::document::ReferenceLines::default(),
            embed_in_pdf: false,
        },
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".scratch")
            .join(format!("pdf-save-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        for entry in fs::read_dir(&self.0).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        fs::remove_dir(&self.0).unwrap();
    }
}

fn fixture(rotation: i32) -> Vec<u8> {
    let mut document = PdfDocument::new();
    let mut page = document
        .new_page_at(0, mupdf::Size::new(600., 800.))
        .unwrap();
    let mut object = page.object();
    object
        .dict_put(
            "MediaBox",
            numbers(&document, &[-30., -40., 600., 800.]).unwrap(),
        )
        .unwrap();
    object
        .dict_put(
            "CropBox",
            numbers(&document, &[20., 30., 520., 730.]).unwrap(),
        )
        .unwrap();
    object
        .dict_put("UserUnit", PdfObject::new_real(1.5).unwrap())
        .unwrap();
    page.set_rotation(rotation).unwrap();
    let contents = b"0.8 0.3 0.1 rg 50 60 40 50 re f\n";
    let stream = document
        .add_stream(&Buffer::from_bytes(contents).unwrap(), None, false)
        .unwrap();
    object.dict_put("Contents", stream).unwrap();
    let mut annotation = page
        .add_text_annotation(mupdf::Rect::new(10., 10., 30., 30.), "Existing note")
        .unwrap();
    annotation.update().unwrap();
    // An existing URI link and a simple bookmark must survive full-copy output.
    let link = document.new_object_from_str("<< /Type /Annot /Subtype /Link /Rect [50 60 90 110] /A << /S /URI /URI (https://example.com/) >> >>").unwrap();
    let link = document.add_object(&link).unwrap();
    required(&object, "Annots")
        .unwrap()
        .array_push(link)
        .unwrap();
    let mut outlines = document.new_dict().unwrap();
    outlines
        .dict_put("Type", PdfObject::new_name("Outlines").unwrap())
        .unwrap();
    let outline_ref = document.add_object(&outlines).unwrap();
    let mut bookmark = document.new_dict().unwrap();
    bookmark
        .dict_put("Title", PdfObject::new_string("Existing bookmark").unwrap())
        .unwrap();
    bookmark.dict_put("Parent", outline_ref.clone()).unwrap();
    let mut dest = document.new_array().unwrap();
    dest.array_push(page.object()).unwrap();
    dest.array_push(PdfObject::new_name("Fit").unwrap())
        .unwrap();
    bookmark.dict_put("Dest", dest).unwrap();
    let bookmark = document.add_object(&bookmark).unwrap();
    outlines.dict_put("First", bookmark.clone()).unwrap();
    outlines.dict_put("Last", bookmark).unwrap();
    outlines
        .dict_put("Count", PdfObject::new_int(1).unwrap())
        .unwrap();
    document
        .catalog()
        .unwrap()
        .dict_put("Outlines", outline_ref)
        .unwrap();
    document
        .new_page_at(1, mupdf::Size::new(300., 400.))
        .unwrap();
    document
        .add_embedded_file(
            "existing.txt",
            b"keep this attachment",
            EmbeddedFileOptions::new("existing.txt"),
        )
        .unwrap();
    let mut bytes = Vec::new();
    document.write_to(&mut bytes).unwrap();
    bytes
}

fn stroke_for(bytes: &[u8]) -> StrokeRecord {
    let document = PdfDocument::from_bytes(bytes).unwrap();
    pressure_stroke(page_geometry(&document.load_pdf_page(0).unwrap()).unwrap()).unwrap()
}

fn save_stroke(base: &[u8], stroke: &StrokeRecord, output: &Path) -> Result<()> {
    let (bytes, data) = serialize(base, &[std::sync::Arc::new(stroke.clone())], Uuid::new_v4())?;
    crate::storage::save_atomic(output, None, &bytes, |path| {
        verify_data(&read_pdf(path)?, &data)
    })?;
    Ok(())
}

fn saved_fixture(directory: &TestDirectory) -> Vec<u8> {
    let base = fixture(0);
    let output = directory.file("pressure.pdf");
    save_stroke(&base, &stroke_for(&base), &output).unwrap();
    fs::read(output).unwrap()
}

#[test]
fn session_snapshots_save_multiple_pages_repeat_without_duplicates_and_remove_undone_ink() {
    use std::sync::Arc;
    let base = fixture(270);
    let original = PdfDocument::from_bytes(&base).unwrap();
    let first = Arc::new(stroke_for(&base));
    let third = Arc::new(stroke_for(&base));
    let mut second =
        pressure_stroke(page_geometry(&original.load_pdf_page(1).unwrap()).unwrap()).unwrap();
    second.page_index = 1;
    let second = Arc::new(second);
    let document_id = Uuid::new_v4();
    for _ in 0..2 {
        let (bytes, data) = serialize(
            &base,
            &[first.clone(), third.clone(), second.clone()],
            document_id,
        )
        .unwrap();
        assert_eq!(data.document_id, document_id);
        assert_eq!(data.strokes.len(), 3);
        let reopened = PdfDocument::from_bytes(&bytes).unwrap();
        for index in 0..2 {
            assert_eq!(
                reopened.load_pdf_page(index).unwrap().annotations().count(),
                original.load_pdf_page(index).unwrap().annotations().count()
                    + if index == 0 { 2 } else { 1 }
            );
        }
        assert_eq!(
            reopened
                .load_embedded_file("existing.txt")
                .unwrap()
                .unwrap(),
            b"keep this attachment"
        );
    }
    let (bytes, data) = serialize(&base, &[second], document_id).unwrap();
    assert_eq!(data.strokes.len(), 1);
    let reopened = PdfDocument::from_bytes(&bytes).unwrap();
    assert_eq!(
        reopened.load_pdf_page(0).unwrap().annotations().count(),
        original.load_pdf_page(0).unwrap().annotations().count()
    );
    // A reopened editor supplies all surviving objects, including loaded ink.
    let mut snapshot: Vec<_> = data
        .strokes
        .iter()
        .map(|s| Arc::new(s.stroke.clone()))
        .collect();
    snapshot.push(first);
    let (_, extended) = serialize(&bytes, &snapshot, Uuid::new_v4()).unwrap();
    assert_eq!(extended.document_id, document_id);
    assert_eq!(extended.strokes.len(), 2);
    let (_, empty) = serialize(&base, &[], document_id).unwrap();
    assert!(empty.strokes.is_empty());
}

#[test]
fn ignored_private_metadata_can_be_replaced_without_removing_standard_ink() {
    let directory = TestDirectory::new();
    let bytes = saved_fixture(&directory);
    let mut document = PdfDocument::from_bytes(&bytes).unwrap();
    let filename = read_edit_data(&document).unwrap().unwrap().filename();
    document
        .add_embedded_file(
            &filename,
            b"invalid metadata",
            EmbeddedFileOptions::new(&filename),
        )
        .unwrap();
    let mut corrupt = Vec::new();
    document.write_to(&mut corrupt).unwrap();
    let fresh = std::sync::Arc::new(stroke_for(&corrupt));
    let (bytes, data) = serialize(&corrupt, &[fresh], Uuid::new_v4()).unwrap();
    assert_eq!(data.strokes.len(), 1);
    let reopened = PdfDocument::from_bytes(&bytes).unwrap();
    assert_eq!(
        reopened.load_pdf_page(0).unwrap().annotations().count(),
        document.load_pdf_page(0).unwrap().annotations().count() + 1
    );
    assert_eq!(
        attachment_names(&reopened)
            .unwrap()
            .iter()
            .filter(|name| name.starts_with("naive-note."))
            .count(),
        1
    );
}

#[test]
fn draft_serializes_its_original_paper_geometry_and_configurable_background() {
    let geometry = PageGeometry::new(
        crate::document::Rect::new(20., 30., 615., 872.),
        crate::document::Rotation::R0,
        1.,
    )
    .unwrap();
    let base = draft_pdf(geometry).unwrap();
    let document = PdfDocument::from_bytes(&base).unwrap();
    let page = document.load_pdf_page(0).unwrap();
    assert_eq!(page_geometry(&page).unwrap(), geometry);
    let (bytes, data) = serialize_with_reference(
        &base,
        &[std::sync::Arc::new(pressure_stroke(geometry).unwrap())],
        Uuid::new_v4(),
        ReferenceLayer {
            lines: crate::document::ReferenceLines::default(),
            embed_in_pdf: true,
        },
    )
    .unwrap();
    assert_eq!(data.strokes.len(), 1);
    let reopened = PdfDocument::from_bytes(&bytes).unwrap();
    assert_eq!(
        required(&reopened.load_pdf_page(0).unwrap().object(), "Contents")
            .unwrap()
            .len()
            .unwrap(),
        2
    );
}

#[test]
fn strokes_can_leave_the_page_and_return_without_changing_samples_on_save() {
    use crate::{
        document::{CanvasViewport, PenPhase, PenStyle, Rect, ViewTransform},
        ink::{LiveStroke, StrokeTarget},
    };
    let directory = TestDirectory::new();
    for rotation in [0, 90, 180, 270] {
        let base = fixture(rotation);
        let document = PdfDocument::from_bytes(&base).unwrap();
        let geometry = page_geometry(&document.load_pdf_page(0).unwrap()).unwrap();
        let size = geometry.paper_size();
        let target = StrokeTarget {
            page_index: 0,
            original: geometry,
            displayed: geometry,
            view: ViewTransform::default(),
            viewport: CanvasViewport::new(Rect::new(0., 0., 1600., 1600.), 1., (1600, 1600))
                .unwrap(),
        };
        for smoothing in [false, true] {
            let mut live = LiveStroke::new(target, smoothing, PenStyle::default());
            // Start inside, cross every edge, then return to the original page.
            let positions = [
                Point::new(30., 40.),
                Point::new(-40., 50.),
                Point::new(40., 60.),
                Point::new(50., -40.),
                Point::new(60., 70.),
                Point::new(size.width + 40., 80.),
                Point::new(size.width - 30., 90.),
                Point::new(100., size.height + 40.),
                Point::new(110., size.height - 30.),
            ];
            for (index, point) in positions.into_iter().enumerate() {
                let phase = if index == 0 {
                    PenPhase::Down
                } else if index == positions.len() - 1 {
                    PenPhase::Up
                } else {
                    PenPhase::Move
                };
                live.push_mouse(
                    target
                        .viewport
                        .logical_to_physical(target.view.paper_to_logical(point)),
                    index as u64 * 100_000,
                    phase,
                );
            }
            let stroke = live.finish().unwrap();
            assert_eq!(stroke.processing.modeler.is_some(), smoothing);
            assert!(
                stroke
                    .raw_samples
                    .iter()
                    .any(|sample| !geometry.contains_paper(sample.position))
            );
            assert!(
                stroke
                    .modeled_samples
                    .iter()
                    .any(|sample| !geometry.contains_paper(sample.position))
            );
            let output = directory.file(&format!("crossing-{rotation}-{smoothing}.pdf"));
            save_stroke(&base, &stroke, &output).unwrap();
            let bytes = fs::read(output).unwrap();
            let reopened = PdfDocument::from_bytes(&bytes).unwrap();
            assert_eq!(
                page_geometry(&reopened.load_pdf_page(0).unwrap()).unwrap(),
                geometry
            );
            let data = read_edit_data(&reopened).unwrap().unwrap();
            assert_eq!(data.strokes.len(), 1);
            assert_eq!(data.strokes[0].stroke, *stroke);
            let snapshot = data
                .strokes
                .iter()
                .map(|s| std::sync::Arc::new(s.stroke.clone()))
                .collect::<Vec<_>>();
            let (_, resaved) = serialize(&bytes, &snapshot, Uuid::new_v4()).unwrap();
            assert_eq!(resaved, data);
        }
    }
}

#[test]
fn pressure_ink_attachment_round_trips_with_rotated_cropped_scaled_pages_and_existing_content() {
    let directory = TestDirectory::new();
    for rotation in [0, 90, 180, 270] {
        let base = fixture(rotation);
        let before = PdfDocument::from_bytes(&base).unwrap();
        let stroke = stroke_for(&base);
        let output = directory.file(&format!("中文 压感 {rotation}.pdf"));
        save_stroke(&base, &stroke, &output).unwrap();
        let document = PdfDocument::from_bytes(&fs::read(output).unwrap()).unwrap();
        let data = read_edit_data(&document).unwrap().unwrap();
        assert_eq!(data.strokes.len(), 1);
        assert_eq!(data.strokes[0].stroke, stroke);
        assert_eq!(document.page_count().unwrap(), 2);
        assert_eq!(
            document
                .load_embedded_file("existing.txt")
                .unwrap()
                .unwrap(),
            b"keep this attachment"
        );
        let page = document.load_pdf_page(0).unwrap();
        assert_eq!(page.annotations().count(), 2); // Link is stored separately by MuPDF.
        assert_eq!(
            required(&page.object(), "Annots").unwrap().len().unwrap(),
            required(&before.load_pdf_page(0).unwrap().object(), "Annots")
                .unwrap()
                .len()
                .unwrap()
                + 1
        );
        let links: Vec<_> = page.link_annotations().unwrap().collect();
        assert_eq!(links.len(), 1);
        assert_eq!(
            required(&page.object(), "Contents")
                .unwrap()
                .read_stream()
                .unwrap(),
            required(&before.load_pdf_page(0).unwrap().object(), "Contents")
                .unwrap()
                .read_stream()
                .unwrap()
        );
        assert_eq!(
            required(
                &required(
                    &required(&document.catalog().unwrap(), "Outlines").unwrap(),
                    "First"
                )
                .unwrap(),
                "Title"
            )
            .unwrap()
            .as_string()
            .unwrap(),
            "Existing bookmark"
        );
        // Reopen, update and serialize again: MuPDF must keep the exact custom AP.
        document.load_pdf_page(0).unwrap().update().unwrap();
        let mut rewritten = Vec::new();
        document.write_to(&mut rewritten).unwrap();
        assert_eq!(
            read_edit_data(&PdfDocument::from_bytes(&rewritten).unwrap()).unwrap(),
            Some(data)
        );
    }
}

#[test]
fn same_snapshot_saves_are_stable_and_existing_paths_require_a_known_fingerprint() {
    let directory = TestDirectory::new();
    let base = fixture(90);
    let stroke = stroke_for(&base);
    let source = directory.file("source.pdf");
    fs::write(&source, &base).unwrap();
    assert!(save_stroke(&base, &stroke, &source).is_err());
    assert_eq!(fs::read(&source).unwrap(), base);
    for name in ["first.pdf", "second.pdf"] {
        let path = directory.file(name);
        save_stroke(&base, &stroke, &path).unwrap();
        assert!(save_stroke(&base, &stroke, &path).is_err());
        let document = PdfDocument::from_bytes(&fs::read(path).unwrap()).unwrap();
        assert_eq!(
            read_edit_data(&document).unwrap().unwrap().strokes[0]
                .stroke
                .id,
            stroke.id
        );
        assert_eq!(document.load_pdf_page(0).unwrap().annotations().count(), 2);
    }
    // Saving retains no source file handle.
    fs::rename(&source, directory.file("renamed.pdf")).unwrap();
}

#[test]
fn reopened_snapshot_replaces_verified_ink_preserves_foreign_objects_and_document_uuid() {
    let directory = TestDirectory::new();
    let first = saved_fixture(&directory);
    let previous = read_edit_data(&PdfDocument::from_bytes(&first).unwrap())
        .unwrap()
        .unwrap();
    let old = std::sync::Arc::new(previous.strokes[0].stroke.clone());
    let fresh = std::sync::Arc::new(stroke_for(&first));
    let (bytes, _) = serialize(&first, &[old.clone(), fresh.clone()], Uuid::new_v4()).unwrap();
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let data = read_edit_data(&document).unwrap().unwrap();
    assert_eq!(data.document_id, previous.document_id);
    assert_eq!(data.strokes.len(), 2);
    assert_eq!(data.strokes[0], previous.strokes[0]);
    assert_eq!(document.embedded_files().unwrap().len(), 2);
    assert!(serialize(&first, &[old.clone(), old], Uuid::new_v4()).is_err());
    // Deleting every native object also removes its standard annotation.
    let (empty, data) = serialize(&bytes, &[], Uuid::new_v4()).unwrap();
    assert!(data.strokes.is_empty());
    let document = PdfDocument::from_bytes(&empty).unwrap();
    assert_eq!(document.load_pdf_page(0).unwrap().annotations().count(), 1);
    assert_eq!(
        document
            .load_embedded_file("existing.txt")
            .unwrap()
            .unwrap(),
        b"keep this attachment"
    );
    // Undo restores the same native record over the original opened base.
    let (_, data) = serialize(&first, &[fresh], Uuid::new_v4()).unwrap();
    assert_eq!(data.strokes.len(), 1);
}

#[test]
fn highlighter_appearance_round_trips_blending_and_detects_resource_tampering() {
    use crate::{
        document::{CanvasViewport, PenPhase, PenStyle, ViewTransform},
        ink::{LiveStroke, StrokeTarget},
    };
    let base = fixture(90);
    let document = PdfDocument::from_bytes(&base).unwrap();
    let geometry = page_geometry(&document.load_pdf_page(0).unwrap()).unwrap();
    let target = StrokeTarget {
        page_index: 0,
        original: geometry,
        displayed: geometry,
        view: ViewTransform::default(),
        viewport: CanvasViewport::new(
            crate::document::Rect::new(0., 0., 1000., 1000.),
            1.,
            (1000, 1000),
        )
        .unwrap(),
    };
    let mut ink = LiveStroke::new(target, false, PenStyle::highlighter());
    ink.push_mouse(Point::new(100., 100.), 0, PenPhase::Down);
    ink.push_mouse(Point::new(200., 100.), 1000, PenPhase::Up);
    let stroke = ink.finish().unwrap();
    let (bytes, data) = serialize(&base, std::slice::from_ref(&stroke), Uuid::new_v4()).unwrap();
    assert_eq!(data.strokes[0].stroke, *stroke);
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let data = read_edit_data(&document).unwrap().unwrap();
    assert_eq!(data.strokes[0].stroke.style.kind, InkKind::Highlighter);
    let object = document
        .load_pdf_page(0)
        .unwrap()
        .annotations()
        .last()
        .unwrap()
        .object();
    let ap = normal_appearance(&object).unwrap();
    let path = String::from_utf8(ap.read_stream().unwrap()).unwrap();
    assert_eq!(path.matches("/NNHighlight gs").count(), 1);
    assert!(path.ends_with("f\nQ\n"));
    let mut state = required(
        &required(&required(&ap, "Resources").unwrap(), "ExtGState").unwrap(),
        "NNHighlight",
    )
    .unwrap();
    assert_eq!(
        required(&state, "BM").unwrap().as_name().unwrap(),
        b"Multiply"
    );
    assert!((required(&state, "ca").unwrap().as_float().unwrap() - 0.4).abs() < 1e-6);
    state
        .dict_put("ca", PdfObject::new_real(1.0).unwrap())
        .unwrap();
    assert!(read_edit_data(&document).is_err());
}

#[test]
fn external_changes_to_id_centerline_rectangle_flags_and_appearance_invalidate_edit_records() {
    let directory = TestDirectory::new();
    let bytes = saved_fixture(&directory);
    for key in ["NM", "InkList", "Rect", "F", "AP"] {
        let document = PdfDocument::from_bytes(&bytes).unwrap();
        let page = document.load_pdf_page(0).unwrap();
        let mut annotation = page.annotations().last().unwrap().object();
        match key {
            "NM" => annotation
                .dict_put(key, PdfObject::new_string("external-id").unwrap())
                .unwrap(),
            "F" => annotation
                .dict_put(key, PdfObject::new_int(0).unwrap())
                .unwrap(),
            "InkList" => {
                required(&annotation, key)
                    .unwrap()
                    .get_array(0)
                    .unwrap()
                    .unwrap()
                    .array_put(0, PdfObject::new_real(999.).unwrap())
                    .unwrap();
            }
            "Rect" => {
                required(&annotation, key)
                    .unwrap()
                    .array_put(0, PdfObject::new_real(999.).unwrap())
                    .unwrap();
            }
            "AP" => {
                normal_appearance(&annotation)
                    .unwrap()
                    .write_stream_string("q 1 0 0 rg 0 0 10 10 re f Q")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(read_edit_data(&document).is_err(), "{key}");
        assert!(edit_data_status(&document).starts_with("Edit data ignored:"));
        assert_eq!(document.page_count().unwrap(), 2);
    }
}

#[test]
fn missing_corrupt_or_future_edit_data_does_not_block_pdf_parsing() {
    let directory = TestDirectory::new();
    let bytes = saved_fixture(&directory);
    for variant in ["missing", "corrupt", "future"] {
        let mut document = PdfDocument::from_bytes(&bytes).unwrap();
        let mut data = read_edit_data(&document).unwrap().unwrap();
        let name = data.filename();
        if variant == "missing" {
            document.delete_embedded_file(&name).unwrap();
        } else {
            let payload = if variant == "corrupt" {
                b"invalid zstd data".to_vec()
            } else {
                data.format_version = 999;
                data.encode().unwrap()
            };
            document
                .add_embedded_file(&name, &payload, EmbeddedFileOptions::new(&name))
                .unwrap();
        }
        let mut saved = Vec::new();
        document.write_to(&mut saved).unwrap();
        let reopened = PdfDocument::from_bytes(&saved).unwrap();
        assert_eq!(reopened.page_count().unwrap(), 2);
        assert_eq!(reopened.load_pdf_page(0).unwrap().annotations().count(), 2);
        if variant == "missing" {
            assert!(read_edit_data(&reopened).unwrap().is_none());
        } else {
            assert!(edit_data_status(&reopened).starts_with("Edit data ignored:"));
        }
    }
}

#[test]
fn embedded_name_tree_kids_are_preserved_and_flat_names_are_sorted() {
    let directory = TestDirectory::new();
    let base = fixture(0);
    let document = PdfDocument::from_bytes(&base).unwrap();
    let mut names = required(&document.catalog().unwrap(), "Names").unwrap();
    let leaf = required(&names, "EmbeddedFiles").unwrap();
    let mut kids = document.new_array().unwrap();
    kids.array_push(leaf).unwrap();
    let mut tree = document.new_dict().unwrap();
    tree.dict_put("Kids", kids).unwrap();
    names.dict_put("EmbeddedFiles", tree).unwrap();
    let mut bytes = Vec::new();
    document.write_to(&mut bytes).unwrap();
    let output = directory.file("kids.pdf");
    save_stroke(&bytes, &stroke_for(&bytes), &output).unwrap();
    let saved = PdfDocument::from_bytes(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        saved.load_embedded_file("existing.txt").unwrap().unwrap(),
        b"keep this attachment"
    );
    let files = saved.embedded_files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].name < files[1].name);
}

#[test]
fn signed_encrypted_and_invalid_records_are_rejected_before_creating_output() {
    let directory = TestDirectory::new();
    let base = fixture(0);
    let mut document = PdfDocument::from_bytes(&base).unwrap();
    let signature = document
        .new_object_from_str("<< /Type /Sig /ByteRange [0 1 2 3] >>")
        .unwrap();
    let signature = document.add_object(&signature).unwrap();
    document
        .catalog()
        .unwrap()
        .dict_put("TestSignature", signature)
        .unwrap();
    let mut bytes = Vec::new();
    document.write_to(&mut bytes).unwrap();
    let output = directory.file("rejected.pdf");
    assert!(
        save_stroke(&bytes, &stroke_for(&base), &output)
            .unwrap_err()
            .to_string()
            .contains("Signed PDF")
    );
    assert!(!output.exists());
    let mut options = mupdf::pdf::PdfWriteOptions::default();
    options
        .set_encryption(mupdf::pdf::Encryption::Aes256)
        .set_owner_password("owner")
        .set_user_password("password");
    let mut encrypted = Vec::new();
    PdfDocument::from_bytes(&base)
        .unwrap()
        .write_to_with_options(&mut encrypted, options)
        .unwrap();
    assert!(save_stroke(&encrypted, &stroke_for(&base), &output).is_err());
    assert!(!output.exists());
    let mut stroke = stroke_for(&base);
    stroke.modeled_samples[0].pressure = f64::NAN;
    assert!(save_stroke(&base, &stroke, &output).is_err());
    assert!(!output.exists());
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e12] {
        let mut stroke = stroke_for(&base);
        stroke.modeled_samples[0].position.x = invalid;
        assert!(save_stroke(&base, &stroke, &output).is_err());
        assert!(!output.exists());
    }
}

#[test]
fn bounded_zstd_cbor_decoder_rejects_truncation_trailing_data_and_large_output() {
    let directory = TestDirectory::new();
    let bytes = saved_fixture(&directory);
    let data = read_edit_data(&PdfDocument::from_bytes(&bytes).unwrap())
        .unwrap()
        .unwrap();
    let encoded = data.encode().unwrap();
    assert_eq!(EditData::decode(&encoded).unwrap(), data);
    assert!(EditData::decode(&encoded[..encoded.len() / 2]).is_err());
    let mut cbor = Vec::new();
    ciborium::into_writer(&data, &mut cbor).unwrap();
    cbor.push(0);
    assert!(EditData::decode(&zstd::stream::encode_all(cbor.as_slice(), 3).unwrap()).is_err());
    let bomb = zstd::stream::encode_all(vec![0; MAX_EDIT_BYTES + 1].as_slice(), 1).unwrap();
    assert!(EditData::decode(&bomb).is_err());
}

fn page_content(document: &PdfDocument, page: i32, references: bool) -> Vec<Vec<u8>> {
    let object = document.load_pdf_page(page).unwrap().object();
    let Some(contents) = object.get_dict("Contents").unwrap() else {
        return Vec::new();
    };
    let streams: Vec<_> = if contents.is_array().unwrap() {
        contents.array_iter().unwrap().map(Result::unwrap).collect()
    } else {
        vec![contents]
    };
    streams
        .into_iter()
        .filter(|stream| {
            stream
                .get_dict("NaiveNoteReferenceLines")
                .unwrap()
                .is_some()
                == references
        })
        .map(|stream| stream.read_stream().unwrap())
        .collect()
}

#[test]
fn reference_vectors_roundtrip_replace_without_duplicates_and_preserve_source_content() {
    use crate::document::{ReferenceLines, ReferencePattern};
    for rotation in [0, 90, 180, 270] {
        let base = fixture(rotation);
        let source = PdfDocument::from_bytes(&base).unwrap();
        for (pattern, _) in ReferencePattern::ALL {
            let layer = ReferenceLayer {
                lines: ReferenceLines {
                    pattern,
                    width: 32.0,
                    height: 24.0,
                    color_rgb: [120, 150, 180],
                },
                embed_in_pdf: true,
            };
            let (bytes, data) =
                serialize_with_reference(&base, &[], Uuid::new_v4(), layer).unwrap();
            assert_eq!(data.reference_lines, Some(layer));
            let (bytes, _) = serialize_with_reference(&bytes, &[], Uuid::new_v4(), layer).unwrap();
            let saved = PdfDocument::from_bytes(&bytes).unwrap();
            for page in 0..saved.page_count().unwrap() {
                assert_eq!(
                    page_content(&saved, page, false),
                    page_content(&source, page, false)
                );
                assert_eq!(
                    page_content(&saved, page, true).len(),
                    if pattern == ReferencePattern::None {
                        0
                    } else {
                        2
                    }
                );
            }
            // Turning embedding off removes our old vectors, while retaining
            // document metadata and all source streams across every page.
            let overlay = ReferenceLayer {
                embed_in_pdf: false,
                ..layer
            };
            let (bytes, data) =
                serialize_with_reference(&bytes, &[], Uuid::new_v4(), overlay).unwrap();
            assert_eq!(data.reference_lines, Some(overlay));
            let saved = PdfDocument::from_bytes(&bytes).unwrap();
            for page in 0..saved.page_count().unwrap() {
                assert!(page_content(&saved, page, true).is_empty());
                assert_eq!(
                    page_content(&saved, page, false),
                    page_content(&source, page, false)
                );
            }
        }
    }
}

#[test]
fn legacy_edit_attachment_without_reference_metadata_uses_the_fallback() {
    #[derive(serde::Serialize)]
    struct LegacyEditData {
        format_version: u32,
        document_id: Uuid,
        strokes: Vec<SavedStroke>,
    }
    let mut cbor = Vec::new();
    ciborium::into_writer(
        &LegacyEditData {
            format_version: FORMAT_VERSION,
            document_id: Uuid::new_v4(),
            strokes: Vec::new(),
        },
        &mut cbor,
    )
    .unwrap();
    let data = EditData::decode(&zstd::stream::encode_all(cbor.as_slice(), 3).unwrap()).unwrap();
    assert!(data.reference_lines.is_none());
    let mut invalid = data;
    invalid.reference_lines = Some(ReferenceLayer {
        lines: crate::document::ReferenceLines {
            width: f64::NAN,
            ..crate::document::ReferenceLines::default()
        },
        embed_in_pdf: false,
    });
    assert!(EditData::decode(&invalid.encode().unwrap()).is_err());
}
