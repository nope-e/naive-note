//! MuPDF stays on one worker; only owned metadata and pixels cross threads.

mod backend;
mod cache;
mod edit_data;
pub mod probe;
mod save;
mod tiles;
mod worker;

use std::{path::PathBuf, sync::Arc};

use crate::document::{PageGeometry, Rect};

pub use cache::ByteLru;
pub use tiles::{PageTile, TileKey, visible_tiles};
pub use worker::PdfWorker;

pub const PAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub const TILE_CACHE_BYTES: usize = 128 * 1024 * 1024;

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
    pub pages: Vec<PageGeometry>,
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
    pub page: u32,
    pub previews: Vec<(u32, Arc<PageBitmap>)>,
    pub complete: bool,
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
