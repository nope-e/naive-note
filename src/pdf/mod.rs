//! MuPDF stays on one worker; only owned metadata and pixels cross threads.

mod backend;
mod worker;

use std::{path::PathBuf, sync::Arc};

use crate::document::PageGeometry;

pub use worker::PdfWorker;

pub struct PageBitmap {
    pub width: u32,
    pub height: u32,
    /// Top-down, tightly packed RGBA8; opaque white page background.
    pub rgba: Vec<u8>,
    /// Geometry when rasterized, including the PDF's original rotation.
    pub geometry: PageGeometry,
}

pub struct LoadedPdf {
    pub path: PathBuf,
    pub page_count: u32,
    pub first_page: Arc<PageBitmap>,
}

pub struct OpenResponse {
    pub request: u64,
    pub result: Result<LoadedPdf, String>,
}
