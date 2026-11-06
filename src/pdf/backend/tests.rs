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
        let session = PdfSession::from_bytes(&pdf_bytes(attributes)).unwrap();
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
