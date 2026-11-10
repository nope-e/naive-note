//! MuPDF stays on one worker; only owned metadata and pixels cross threads.

mod backend;
mod cache;
mod edit_data;
mod save;
mod tiles;
mod worker;

use std::{path::PathBuf, sync::Arc};

use crate::{
    document::{PageGeometry, Rect, StrokeRecord},
    storage::FileFingerprint,
};

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
    pub fingerprint: FileFingerprint,
}

pub struct SaveSnapshot {
    pub document: Option<u64>,
    pub revision: u64,
    pub strokes: Vec<Arc<StrokeRecord>>,
    pub draft_page: PageGeometry,
    pub document_id: uuid::Uuid,
    pub path: PathBuf,
    /// Most recent successful save, or the file bytes at open time.
    pub tracked_target: Option<(PathBuf, FileFingerprint)>,
    /// A new recovery copy must never adopt an existing file as its baseline.
    pub require_new_target: bool,
}

pub struct SavedPdf {
    pub path: PathBuf,
    pub fingerprint: FileFingerprint,
}

pub struct SaveResponse {
    pub document: Option<u64>,
    pub request: u64,
    pub revision: u64,
    pub result: Result<SavedPdf, SaveError>,
}

#[derive(Debug)]
pub struct SaveError {
    pub message: String,
    pub conflict: bool,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<String> for SaveError {
    fn from(message: String) -> Self {
        Self {
            message,
            conflict: false,
        }
    }
}

impl From<&str> for SaveError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
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

pub enum PdfResponse {
    Opened(OpenResponse),
    Tiles(TileResponse),
    Saved(SaveResponse),
    DraftRemoved {
        path: PathBuf,
        result: Result<(), String>,
    },
}
