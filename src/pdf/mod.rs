//! MuPDF stays on one worker; only owned metadata and pixels cross threads.

mod backend;
mod edit_data;
pub mod probe;
mod save;
mod tiles;
mod worker;

use std::{path::PathBuf, sync::Arc};

use crate::document::{PageGeometry, Rect};

pub use tiles::{PageTile, TileCache, TileKey, visible_tiles};
pub use worker::PdfWorker;

pub struct PageBitmap {
    pub width: u32,
    pub height: u32,
    /// Top-down, tightly packed RGBA8; opaque white page background.
    pub rgba: Arc<[u8]>,
    /// Geometry when rasterized, including the PDF's original rotation.
    pub geometry: PageGeometry,
    /// Image extent in the original, rotated paper coordinate system.
    pub bounds: Rect,
}

pub struct LoadedPdf {
    pub path: PathBuf,
    pub page_count: u32,
    pub first_page: Arc<PageBitmap>,
    pub edit_data_status: String,
}

pub struct OpenResponse {
    pub request: u64,
    pub result: Result<LoadedPdf, String>,
}

pub struct TileResponse {
    pub document: u64,
    pub request: u64,
    pub result: Result<Vec<Arc<PageTile>>, String>,
}

pub struct ExportResponse {
    pub document: u64,
    pub request: u64,
    pub result: Result<std::path::PathBuf, String>,
}

pub enum PdfResponse {
    Opened(OpenResponse),
    Tiles(TileResponse),
    Exported(ExportResponse),
}
