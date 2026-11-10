//! Headless parsing, geometry and bitmap-layout checks. No window, GPU, image
//! export, pixel inspection or visual acceptance is performed here.

use super::*;
use crate::document::{Point, Size};

fn pdf_bytes(attributes: &str) -> Vec<u8> {
    let contents = "0.2 0.6 0.8 rg 30 40 80 120 re f\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!("<< /Type /Pages /Kids [3 0 R] /Count 1 {attributes} >>"),
        "<< /Type /Page /Parent 2 0 R /UserUnit 1.5 /Contents 4 0 R >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{contents}endstream",
            contents.len()
        ),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    bytes
}

#[test]
fn effective_geometry_matches_mupdf_for_inherited_boxes_rotation_and_user_unit() {
    for (degrees, expected) in [
        (0, Rotation::R0),
        (90, Rotation::R90),
        (180, Rotation::R180),
        (270, Rotation::R270),
        (-90, Rotation::R270),
        (450, Rotation::R90),
    ] {
        let bytes = pdf_bytes(&format!(
            "/MediaBox [-50 -60 500 600] /CropBox [20 30 220 330] /Rotate {degrees}"
        ));
        let session = PdfSession::from_bytes(&bytes).unwrap();
        let page = session.first_page().unwrap();
        let geometry = page_geometry(&page).unwrap();
        assert_eq!(geometry.crop_box(), Rect::new(20.0, 30.0, 220.0, 330.0));
        assert_eq!(geometry.user_unit(), 1.5);
        assert_eq!(geometry.rotation(), expected);
        assert_eq!(geometry.unrotated_size(), Size::new(300.0, 450.0));
        let ctm = page.ctm().unwrap();
        for point in [
            Point::new(20.0, 30.0),
            Point::new(100.0, 240.0),
            Point::new(220.0, 330.0),
        ] {
            let (x, y) = ctm.transform_xy(point.x as f32, point.y as f32);
            let paper = geometry.pdf_to_paper(point);
            assert!((paper.x - f64::from(x)).abs() < 1e-4);
            assert!((paper.y - f64::from(y)).abs() < 1e-4);
            let round_trip = geometry.paper_to_pdf(paper);
            assert!((round_trip.x - point.x).abs() < 1e-8);
            assert!((round_trip.y - point.y).abs() < 1e-8);
        }
    }
}

#[test]
fn crop_is_intersected_with_media_box_and_missing_crop_uses_media_box() {
    for (attributes, expected) in [
        (
            "/MediaBox [10 20 210 320] /CropBox [-10 -20 300 400]",
            Rect::new(10.0, 20.0, 210.0, 320.0),
        ),
        (
            "/MediaBox [10 20 210 320]",
            Rect::new(10.0, 20.0, 210.0, 320.0),
        ),
    ] {
        let session = PdfSession::from_bytes(&pdf_bytes(attributes)).unwrap();
        assert_eq!(
            page_geometry(&session.first_page().unwrap())
                .unwrap()
                .crop_box(),
            expected
        );
    }
}

#[test]
fn preview_has_bounded_dimensions_and_tightly_packed_rgba() {
    for attributes in [
        "/MediaBox [0 0 200 300] /Rotate 90",
        "/MediaBox [0 0 20000 30000]",
    ] {
        let mut session = PdfSession::from_bytes(&pdf_bytes(attributes)).unwrap();
        let preview = session.preview(Path::new("memory.pdf")).unwrap();
        let bitmap = preview.first_page;
        assert_eq!(preview.page_count, 1);
        assert!(bitmap.width > 0 && bitmap.width <= MAX_BITMAP_EDGE);
        assert!(bitmap.height > 0 && bitmap.height <= MAX_BITMAP_EDGE);
        assert_eq!(
            bitmap.rgba.len(),
            bitmap.width as usize * bitmap.height as usize * 4
        );
    }
}

#[test]
fn high_resolution_tiles_are_bounded_with_overlap_and_original_page_coordinates() {
    for rotation in [0, 90, 180, 270] {
        let mut session = PdfSession::from_bytes(&pdf_bytes(&format!(
            "/MediaBox [-20 -30 220 330] /CropBox [20 30 220 330] /Rotate {rotation}"
        )))
        .unwrap();
        for key in [
            TileKey {
                page: 0,
                level: 9,
                x: 0,
                y: 0,
            },
            TileKey {
                page: 0,
                level: 9,
                x: 2,
                y: 3,
            },
        ] {
            let tile = session.render_tile(key).unwrap();
            let bitmap = tile.bitmap;
            assert!(bitmap.width > 0 && bitmap.width <= TILE_EDGE + 2 * TILE_OVERLAP);
            assert!(bitmap.height > 0 && bitmap.height <= TILE_EDGE + 2 * TILE_OVERLAP);
            assert_eq!(
                bitmap.rgba.len(),
                bitmap.width as usize * bitmap.height as usize * 4
            );
            assert!(
                (bitmap.bounds.size().width * key.scale() - f64::from(bitmap.width)).abs() < 1e-8
            );
            assert!(
                (bitmap.bounds.size().height * key.scale() - f64::from(bitmap.height)).abs() < 1e-8
            );
            assert_eq!(bitmap.geometry, session.pages[0]);
            let core = key.core();
            assert!(bitmap.bounds.contains(core.min));
            assert!(bitmap.bounds.contains(core.max));
        }
        // Partial last tile must stop at the page's ceil-rounded pixel boundary.
        let size = session.pages[0].paper_size();
        let key = TileKey {
            page: 0,
            level: 9,
            x: (size.width
                * TileKey {
                    page: 0,
                    level: 9,
                    x: 0,
                    y: 0,
                }
                .scale()
                / f64::from(TILE_EDGE))
            .ceil() as u32
                - 1,
            y: 0,
        };
        let tile = session.render_tile(key).unwrap();
        assert!(
            (tile.bitmap.bounds.max.x * key.scale() - (size.width * key.scale()).ceil()).abs()
                < 1e-8
        );
        assert!(
            session
                .render_tile(TileKey {
                    page: 0,
                    level: 9,
                    x: 1000,
                    y: 1000
                })
                .is_err()
        );
        assert!(
            session
                .render_tile(TileKey {
                    page: 0,
                    level: 100,
                    x: 0,
                    y: 0
                })
                .is_err()
        );
    }
}

#[test]
fn worker_refinement_is_superseded_and_preserves_session_after_failed_open() {
    use crate::pdf::{PdfResponse, PdfWorker};
    use std::{sync::mpsc, time::Duration};

    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("mupdf-refine-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("preview.pdf");
    std::fs::write(&path, pdf_bytes("/MediaBox [0 0 200 300]")).unwrap();
    let (responses, received) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        responses.send(response).unwrap();
        let _ = wait.recv_timeout(Duration::from_secs(5));
    })
    .unwrap();
    let document = worker
        .open(path.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    assert!(
        matches!(received.recv_timeout(Duration::from_secs(5)).unwrap(),
        PdfResponse::Opened(response) if response.request == document && response.result.is_ok())
    );
    let key = TileKey {
        page: 0,
        level: 8,
        x: 0,
        y: 0,
    };
    worker.render(document, vec![key]).unwrap();
    let newest = worker
        .render(
            document,
            vec![TileKey {
                page: 0,
                x: 1,
                ..key
            }],
        )
        .unwrap();
    release.send(()).unwrap();
    match received.recv_timeout(Duration::from_secs(5)).unwrap() {
        PdfResponse::Tiles(response) => {
            assert_eq!(response.request, newest);
            assert_eq!(response.document, document);
            assert_eq!(response.result.unwrap()[0].key.x, 1);
        }
        _ => panic!("expected the latest tile job"),
    }
    let failed = worker
        .open(
            directory.join("missing.pdf"),
            crate::document::ReferenceLines::default(),
        )
        .unwrap();
    release.send(()).unwrap();
    assert!(
        matches!(received.recv_timeout(Duration::from_secs(5)).unwrap(),
        PdfResponse::Opened(response) if response.request == failed && response.result.is_err())
    );
    let resumed = worker.render(document, vec![key]).unwrap();
    release.send(()).unwrap();
    assert!(
        matches!(received.recv_timeout(Duration::from_secs(5)).unwrap(),
        PdfResponse::Tiles(response) if response.document == document && response.request == resumed
            && response.result.is_ok())
    );
    worker.render(document, vec![key]).unwrap();
    worker.close();
    release.send(()).unwrap();
    assert!(matches!(
        received.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let replacement = worker
        .open(path.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    assert!(
        matches!(received.recv_timeout(Duration::from_secs(5)).unwrap(),
        PdfResponse::Opened(response) if response.request == replacement && response.result.is_ok())
    );
    worker.render(document, vec![key]).unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        received.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let fresh = worker.render(replacement, vec![key]).unwrap();
    assert!(
        matches!(received.recv_timeout(Duration::from_secs(5)).unwrap(),
        PdfResponse::Tiles(response) if response.document == replacement && response.request == fresh
            && response.result.is_ok())
    );
    release.send(()).unwrap();
    drop(worker);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn snapshot_accepts_unicode_path_and_releases_original_file() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("mupdf-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("中文 PDF 测试.pdf");
    let bytes = pdf_bytes("/MediaBox [0 0 200 300]");
    std::fs::write(&path, &bytes).unwrap();
    let session = PdfSession::open(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(session.document.page_count().unwrap(), 1);
    assert_eq!(
        page_geometry(&session.first_page().unwrap())
            .unwrap()
            .paper_size(),
        Size::new(300.0, 450.0)
    );
    std::fs::remove_dir(&directory).unwrap();
}

#[test]
fn invalid_pdf_and_empty_document_return_errors() {
    assert!(PdfSession::from_bytes(b"not a pdf").is_err());
    let empty = PdfDocument::new();
    let mut bytes = Vec::new();
    empty.write_to(&mut bytes).unwrap();
    assert!(PdfSession::from_bytes(&bytes).is_err());
}

#[test]
fn worker_saves_snapshot_and_keeps_the_immutable_preview_session_available() {
    use crate::{
        ink::test_support::pressure_stroke,
        pdf::{PdfResponse, PdfWorker, SaveSnapshot},
    };
    use std::{sync::mpsc, time::Duration};

    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("mupdf-save-worker-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.pdf");
    let bytes = pdf_bytes("/MediaBox [0 0 200 300] /Rotate 90");
    std::fs::write(&source, &bytes).unwrap();
    let geometry = PdfSession::from_bytes(&bytes).unwrap().pages[0];
    let (sender, receiver) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        sender.send(response).unwrap();
    })
    .unwrap();
    let document = worker
        .open(source.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    assert!(
        matches!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), PdfResponse::Opened(response) if response.result.is_ok())
    );
    let request = worker
        .save(SaveSnapshot {
            document: Some(document),
            revision: 1,
            strokes: vec![Arc::new(pressure_stroke(geometry).unwrap())],
            draft_page: geometry,
            reference_lines: crate::document::ReferenceLayer {
                lines: crate::document::ReferenceLines::default(),
                embed_in_pdf: true,
            },
            document_id: uuid::Uuid::new_v4(),
            path: directory.join("saved.pdf"),
            tracked_target: Some((source.clone(), crate::storage::FileFingerprint::of(&bytes))),
            require_new_target: false,
        })
        .unwrap();
    let output = match receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
        PdfResponse::Saved(response) => {
            assert_eq!(response.document, Some(document));
            assert_eq!(response.request, request);
            response.result.unwrap().path
        }
        _ => panic!("expected PDF save response"),
    };
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    let reopened = PdfDocument::from_bytes(&std::fs::read(&output).unwrap()).unwrap();
    assert!(
        super::super::save::read_edit_data(&reopened)
            .unwrap()
            .is_some()
    );
    // Saving does not mutate the current immutable session or stop refinement.
    worker
        .render(
            document,
            vec![TileKey {
                page: 0,
                level: 0,
                x: 0,
                y: 0,
            }],
        )
        .unwrap();
    assert!(
        matches!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), PdfResponse::Tiles(response) if response.result.is_ok())
    );
    drop(worker);
    std::fs::remove_file(&output).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn saving_a_closed_session_returns_an_error_without_creating_output() {
    use crate::{
        ink::test_support::pressure_stroke,
        pdf::{PdfResponse, PdfWorker, SaveSnapshot},
    };
    use std::{sync::mpsc, time::Duration};
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("mupdf-closed-save-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.pdf");
    let output = directory.join("saved.pdf");
    let bytes = pdf_bytes("/MediaBox [0 0 200 300]");
    std::fs::write(&source, &bytes).unwrap();
    let geometry = PdfSession::from_bytes(&bytes).unwrap().pages[0];
    let (sender, receiver) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        sender.send(response).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
    })
    .unwrap();
    let document = worker
        .open(source.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    assert!(
        matches!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), PdfResponse::Opened(response) if response.result.is_ok())
    );
    worker.close();
    worker
        .save(SaveSnapshot {
            document: Some(document),
            revision: 1,
            strokes: vec![Arc::new(pressure_stroke(geometry).unwrap())],
            draft_page: geometry,
            reference_lines: crate::document::ReferenceLayer {
                lines: crate::document::ReferenceLines::default(),
                embed_in_pdf: true,
            },
            document_id: uuid::Uuid::new_v4(),
            path: output.clone(),
            tracked_target: None,
            require_new_target: false,
        })
        .unwrap();
    release.send(()).unwrap();
    assert!(
        matches!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), PdfResponse::Saved(response) if response.result.is_err())
    );
    assert!(!output.exists());
    release.send(()).unwrap();
    drop(worker);
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

fn multipage_pdf() -> Vec<u8> {
    let mut document = PdfDocument::new();
    for index in 0..6 {
        let mut page = document
            .new_page_at(
                index,
                mupdf::Size::new(200.0 + index as f32 * 20.0, 300.0 + index as f32 * 10.0),
            )
            .unwrap();
        page.set_rotation((index % 4) * 90).unwrap();
        page.object()
            .dict_put(
                "UserUnit",
                mupdf::pdf::PdfObject::new_real(1.0 + index as f32 * 0.25).unwrap(),
            )
            .unwrap();
    }
    let mut bytes = Vec::new();
    document.write_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn mixed_page_previews_and_tiles_use_the_requested_geometry_with_bounded_display_lists() {
    let mut session = PdfSession::from_bytes(&multipage_pdf()).unwrap();
    let loaded = session.preview(Path::new("multi.pdf")).unwrap();
    assert_eq!(loaded.page_count, 6);
    assert_eq!(loaded.pages.len(), 6);
    for index in [5, 1, 4, 2, 0, 3, 5] {
        let geometry = loaded.pages[index as usize];
        let bitmap = session.render_preview(index).unwrap();
        assert_eq!(bitmap.geometry, geometry);
        assert_eq!(bitmap.bounds.size(), geometry.paper_size());
        assert_eq!(
            bitmap.rgba.len(),
            bitmap.width as usize * bitmap.height as usize * 4
        );
        let key = TileKey {
            page: index,
            level: 3,
            x: 0,
            y: 0,
        };
        let tile = session.render_tile(key).unwrap();
        assert_eq!(tile.key.page, index);
        assert_eq!(tile.bitmap.geometry, geometry);
        assert!(session.prepared.len() <= 4);
    }
    assert!(session.render_preview(6).is_err());
    assert!(
        session
            .render_tile(TileKey {
                page: 6,
                level: 3,
                x: 0,
                y: 0
            })
            .is_err()
    );
}

#[test]
fn multipage_worker_delivers_incremental_previews_and_close_cancels_the_rest() {
    use crate::pdf::{PdfResponse, PdfWorker};
    use std::{sync::mpsc, time::Duration};

    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("multipage-worker-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("multi.pdf");
    std::fs::write(&path, multipage_pdf()).unwrap();
    let (send, received) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        send.send(response).unwrap();
        let _ = wait.recv_timeout(Duration::from_secs(5));
    })
    .unwrap();
    let document = worker
        .open(path.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    assert!(
        matches!(received.recv_timeout(Duration::from_secs(5)).unwrap(),
        PdfResponse::Opened(response) if response.result.as_ref().unwrap().pages.len() == 6)
    );
    let request = worker
        .render_pages(document, vec![2, 4], vec![], ReferenceLines::default())
        .unwrap();
    release.send(()).unwrap();
    match received.recv_timeout(Duration::from_secs(5)).unwrap() {
        PdfResponse::Tiles(response) => {
            assert_eq!(response.request, request);
            assert_eq!(response.document, document);
            assert!(!response.complete);
            assert_eq!(response.previews[0].0, 2);
        }
        _ => panic!("expected a page preview"),
    }
    release.send(()).unwrap();
    match received.recv_timeout(Duration::from_secs(5)).unwrap() {
        PdfResponse::Tiles(response) => {
            assert!(response.complete);
            assert_eq!(response.previews[0].0, 4);
        }
        _ => panic!("expected the final preview"),
    }
    let superseded = worker
        .render_pages(document, vec![1], vec![], ReferenceLines::default())
        .unwrap();
    let newest = worker
        .render_pages(document, vec![3, 5], vec![], ReferenceLines::default())
        .unwrap();
    release.send(()).unwrap();
    match received.recv_timeout(Duration::from_secs(5)).unwrap() {
        PdfResponse::Tiles(response) => {
            assert_ne!(response.request, superseded);
            assert_eq!(response.request, newest);
            assert_eq!(response.previews[0].0, 3);
        }
        _ => panic!("expected the latest preview request"),
    }
    worker.close();
    release.send(()).unwrap();
    assert!(matches!(
        received.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(worker);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn saved_reference_vectors_are_removed_only_from_the_live_render_document() {
    use crate::document::{ReferenceLayer, ReferenceLines};
    let source = pdf_bytes("/MediaBox [0 0 200 300]");
    let layer = ReferenceLayer {
        lines: ReferenceLines::default(),
        embed_in_pdf: true,
    };
    let (bytes, _) =
        super::super::save::serialize_with_reference(&source, &[], uuid::Uuid::new_v4(), layer)
            .unwrap();
    let session = PdfSession::from_bytes(&bytes).unwrap();
    assert_eq!(session.base(), bytes);
    assert_eq!(
        super::super::save::read_edit_data(&session.document)
            .unwrap()
            .unwrap()
            .reference_lines,
        Some(layer)
    );
    let contents = session
        .first_page()
        .unwrap()
        .object()
        .get_dict("Contents")
        .unwrap()
        .unwrap();
    assert_eq!(contents.len().unwrap(), 1);
    assert_eq!(
        contents
            .get_array(0)
            .unwrap()
            .unwrap()
            .read_stream()
            .unwrap(),
        b"0.2 0.6 0.8 rg 30 40 80 120 re f\n"
    );
}

#[test]
fn reopened_display_list_places_references_between_source_and_existing_ink() {
    // Inspect vector command order only: no pixels, framebuffer, window or image.
    use crate::document::ReferenceLayer;
    use mupdf::{ColorParams, NativeDevice, StrokeState};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct Commands(Vec<[u8; 3]>);
    impl Commands {
        fn push(&mut self, color: &[f32]) {
            if color.len() == 3 {
                self.0
                    .push(std::array::from_fn(|i| (color[i] * 255.0).round() as u8));
            }
        }
    }
    impl NativeDevice for Commands {
        fn fill_path(
            &mut self,
            _: &mupdf::Path,
            _: bool,
            _: Matrix,
            _: &Colorspace,
            color: &[f32],
            _: f32,
            _: ColorParams,
        ) {
            self.push(color);
        }
        fn stroke_path(
            &mut self,
            _: &mupdf::Path,
            _: &StrokeState,
            _: Matrix,
            _: &Colorspace,
            color: &[f32],
            _: f32,
            _: ColorParams,
        ) {
            self.push(color);
        }
    }
    let lines = ReferenceLines {
        color_rgb: [200, 10, 100],
        ..ReferenceLines::default()
    };
    for rotation in [0, 90, 180, 270] {
        let source = pdf_bytes(&format!(
            "/MediaBox [-20 -30 220 330] /CropBox [10 20 200 300] /Rotate {rotation}"
        ));
        let geometry = PdfSession::from_bytes(&source).unwrap().pages[0];
        let stroke = Arc::new(crate::ink::test_support::pressure_stroke(geometry).unwrap());
        let (bytes, _) = super::super::save::serialize_with_reference(
            &source,
            &[stroke],
            uuid::Uuid::new_v4(),
            ReferenceLayer {
                lines,
                embed_in_pdf: true,
            },
        )
        .unwrap();
        for metadata in [true, false] {
            let bytes = if metadata {
                bytes.clone()
            } else {
                let document = PdfDocument::from_bytes(&bytes).unwrap();
                document.catalog().unwrap().dict_delete("Names").unwrap();
                let mut result = Vec::new();
                document.write_to(&mut result).unwrap();
                result
            };
            let mut session = PdfSession::from_bytes(&bytes).unwrap();
            session.set_reference_lines(lines);
            let commands = Rc::new(RefCell::new(Commands::default()));
            let device = Device::from_native(Rc::clone(&commands)).unwrap();
            let prepared = session.prepare(0).unwrap();
            prepared
                .display_list
                .run(&device, &Matrix::IDENTITY, prepared.bounds)
                .unwrap();
            drop(device);
            let commands = commands.borrow();
            let source = commands
                .0
                .iter()
                .position(|color| *color == [51, 153, 204])
                .unwrap();
            let reference = commands
                .0
                .iter()
                .position(|color| *color == lines.color_rgb)
                .unwrap();
            let ink = commands
                .0
                .iter()
                .position(|color| *color == [25, 105, 190])
                .unwrap();
            assert!(
                source < reference && reference < ink,
                "vector order: {:?}",
                commands.0
            );
            session.set_reference_lines(ReferenceLines {
                pattern: ReferencePattern::None,
                ..lines
            });
            assert!(session.prepared.is_empty());
        }
    }
}
