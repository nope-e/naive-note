//! Large capture-data regression; PDF structure and file IO only.

use std::sync::Arc;

use super::*;
use crate::document::PenPhase;
use crate::pdf::backend::PdfSession;

#[test]
fn large_edit_data_saves_reopens_and_accepts_more_strokes() {
    const OLD_LIMIT: usize = 8 * 1024 * 1024;
    let directory = TestDirectory::new();
    let output = directory.file("large-edit-data.pdf");
    let base = fixture(0);
    let template = stroke_for(&base);
    // Vary real capture attributes deterministically so compression cannot hide
    // the old compressed-attachment limit behind repeated sample data.
    let mut state = 0x1234_5678_9abc_def0_u64;
    let mut random = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1_u64 << 53) as f64
    };
    let mut strokes: Vec<_> = (0..105)
        .map(|_| {
            let mut stroke = template.clone();
            stroke.id = Uuid::new_v4();
            stroke.raw_samples = (0..2500)
                .map(|i| {
                    let mut sample = template.raw_samples[0].clone();
                    sample.position = Point::new(random() * 100., random() * 100.);
                    sample.time_us = i * 1000;
                    sample.pressure = Some(random());
                    sample.tilt_degrees = Some([random() * 180. - 90., random() * 180. - 90.]);
                    sample.rotation_degrees = Some(random() * 360.);
                    sample.phase = match i {
                        0 => PenPhase::Down,
                        2499 => PenPhase::Up,
                        _ => PenPhase::Move,
                    };
                    sample
                })
                .collect();
            Arc::new(stroke)
        })
        .collect();
    let document_id = Uuid::new_v4();
    let (bytes, data) = serialize(&base, &strokes, document_id).unwrap();
    let document = PdfDocument::from_bytes(&bytes).unwrap();
    let attachment = document
        .load_embedded_file(&data.filename())
        .unwrap()
        .unwrap();
    assert!(attachment.len() > OLD_LIMIT);
    assert!(zstd::stream::decode_all(attachment.as_slice()).unwrap().len() > OLD_LIMIT);
    let fingerprint = crate::storage::save_atomic(&output, None, &bytes, |path| {
        verify_data(&read_pdf(path)?, &data)
    })
    .unwrap();
    let session = PdfSession::open(&output).unwrap();
    assert_eq!(session.objects().len(), strokes.len());
    for (loaded, expected) in session.objects().iter().zip(&strokes) {
        assert_eq!(loaded.as_ref(), expected.as_ref());
    }
    // A second save starts from the large PDF and replaces trusted old ink.
    strokes.push(Arc::new(stroke_for(&base)));
    let (extended, expected) = serialize(&read_pdf(&output).unwrap(), &strokes, document_id).unwrap();
    crate::storage::save_atomic(&output, Some(fingerprint), &extended, |path| {
        verify_data(&read_pdf(path)?, &expected)
    })
    .unwrap();
    let reopened = PdfDocument::from_bytes(&read_pdf(&output).unwrap()).unwrap();
    assert_eq!(read_edit_data(&reopened).unwrap().unwrap(), expected);
    let original = PdfDocument::from_bytes(&base).unwrap();
    assert_eq!(
        reopened.load_pdf_page(0).unwrap().annotations().count(),
        original.load_pdf_page(0).unwrap().annotations().count() + strokes.len()
    );
}
