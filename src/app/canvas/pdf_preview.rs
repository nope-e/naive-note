//! Debounce view changes without continuously repainting an idle canvas.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    pdf::{PageTile, PdfWorker, TileCache, TileKey, TileResponse, visible_tiles},
    render::CanvasScene,
};

const REFINE_DELAY: Duration = Duration::from_millis(80);

pub(super) struct PdfPreview {
    document: u64,
    cache: TileCache,
    visible: Vec<TileKey>,
    pending: Option<u64>,
    due: Instant,
    failed: bool,
}

impl PdfPreview {
    pub fn new(document: u64) -> Self {
        Self {
            document,
            cache: TileCache::default(),
            visible: Vec::new(),
            pending: None,
            due: Instant::now(),
            failed: false,
        }
    }

    pub fn cancel(&mut self, worker: &PdfWorker) {
        worker.cancel_render();
        self.pending = None;
    }

    pub fn receive(&mut self, response: TileResponse) -> Result<(), String> {
        if response.document != self.document || self.pending != Some(response.request) {
            return Ok(());
        }
        self.pending = None;
        match response.result {
            Ok(tiles) => {
                for tile in tiles {
                    self.cache.insert(tile);
                }
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }

    pub fn update(
        &mut self,
        context: &egui::Context,
        worker: &PdfWorker,
        scene: &CanvasScene,
        loading: bool,
    ) -> Vec<Arc<PageTile>> {
        let Some(preview) = scene.bitmap.as_ref() else {
            return Vec::new();
        };
        let visible = visible_tiles(preview, scene.page, scene.view, scene.viewport);
        let now = Instant::now();
        if visible != self.visible {
            self.cancel(worker);
            self.visible = visible;
            self.due = now + REFINE_DELAY;
            self.failed = false;
        }
        let mut tiles = Vec::new();
        let mut missing = Vec::new();
        for key in &self.visible {
            if let Some(tile) = self.cache.get(*key) {
                tiles.push(tile);
            } else {
                missing.push(*key);
            }
        }
        if !missing.is_empty() && !loading && !self.failed && self.pending.is_none() {
            if now < self.due {
                context.request_repaint_after(self.due - now);
            } else {
                match worker.render(self.document, missing) {
                    Ok(request) => self.pending = Some(request),
                    Err(error) => {
                        log::warn!("Cannot request PDF detail: {error:#}");
                        self.failed = true;
                    }
                }
            }
        }
        tiles
    }
}
