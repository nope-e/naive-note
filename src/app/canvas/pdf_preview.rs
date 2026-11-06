//! One render queue and bounded pixel caches for all visible PDF pages.

use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    pdf::{
        ByteLru, PAGE_CACHE_BYTES, PageBitmap, PageTile, PdfWorker, TILE_CACHE_BYTES, TileKey,
        TileResponse, visible_tiles,
    },
    render::CanvasScene,
};

const REFINE_DELAY: Duration = Duration::from_millis(80);
const MAX_DETAIL_TILES: usize = 96;

pub(super) struct PdfPreview {
    document: u64,
    tile_cache: ByteLru<TileKey, PageTile>,
    page_cache: ByteLru<u32, PageBitmap>,
    visible: Vec<TileKey>,
    pages: Vec<u32>,
    pending: Option<u64>,
    due: Instant,
    failed: HashSet<u32>,
}

impl PdfPreview {
    pub fn new(document: u64, first_page: Arc<PageBitmap>) -> Self {
        let mut page_cache = ByteLru::new(PAGE_CACHE_BYTES);
        page_cache.insert(0, first_page, |page| page.rgba.len());
        Self {
            document,
            tile_cache: ByteLru::new(TILE_CACHE_BYTES),
            page_cache,
            visible: Vec::new(),
            pages: Vec::new(),
            pending: None,
            due: Instant::now(),
            failed: HashSet::new(),
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
        if response.complete {
            self.pending = None;
        }
        for (page, bitmap) in response.previews {
            self.page_cache.insert(page, bitmap, |page| page.rgba.len());
        }
        match response.result {
            Ok(tiles) => {
                for tile in tiles {
                    self.tile_cache
                        .insert(tile.key, tile, |tile| tile.bitmap.rgba.len());
                }
                Ok(())
            }
            Err(error) => {
                self.failed.insert(response.page);
                Err(error)
            }
        }
    }

    pub fn update(
        &mut self,
        context: &egui::Context,
        worker: &PdfWorker,
        scene: &mut CanvasScene,
        current_page: u32,
        loading: bool,
    ) {
        let mut pages: Vec<_> = scene.pages.iter().map(|page| page.index).collect();
        pages.sort_by_key(|page| page.abs_diff(current_page));
        let mut visible = Vec::new();
        for page in &mut scene.pages {
            page.bitmap = self.page_cache.get(&page.index);
            if let Some(preview) = &page.bitmap {
                visible.extend(visible_tiles(
                    preview,
                    page.page,
                    page.view,
                    scene.viewport,
                    page.index,
                ));
            }
        }
        visible.sort_by_key(|key| key.page.abs_diff(current_page));
        // Share the detail budget across pages; all other pages keep their
        // low-resolution fallback rather than evicting one another's tiles.
        visible.truncate(MAX_DETAIL_TILES);
        let now = Instant::now();
        if pages != self.pages || visible != self.visible {
            // A newly arrived fallback can add detail keys without making the
            // rest of the in-flight preview batch obsolete.
            if pages != self.pages || !self.visible.iter().all(|key| visible.contains(key)) {
                self.cancel(worker);
                self.due = now + REFINE_DELAY;
            }
            self.pages = pages;
            self.visible = visible;
        }
        let missing_pages: Vec<_> = self
            .pages
            .iter()
            .copied()
            .filter(|page| !self.failed.contains(page) && !self.page_cache.contains(page))
            .take(4)
            .collect();
        let mut missing = Vec::new();
        for key in &self.visible {
            if let Some(tile) = self.tile_cache.get(key) {
                if let Some(page) = scene.pages.iter_mut().find(|page| page.index == key.page) {
                    page.tiles.push(tile);
                }
            } else if !self.failed.contains(&key.page) {
                missing.push(*key);
            }
        }
        if (!missing_pages.is_empty() || !missing.is_empty()) && !loading && self.pending.is_none()
        {
            if now < self.due {
                context.request_repaint_after(self.due - now);
            } else {
                match worker.render_pages(self.document, missing_pages, missing) {
                    Ok(request) => self.pending = Some(request),
                    Err(error) => {
                        log::warn!("Cannot request PDF pages: {error:#}");
                        self.failed.extend(&self.pages);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{PageGeometry, Rect, Rotation};

    fn bitmap(bytes: Arc<[u8]>) -> Arc<PageBitmap> {
        Arc::new(PageBitmap {
            width: 1,
            height: 1,
            rgba: bytes,
            geometry: PageGeometry::new(Rect::new(0.0, 0.0, 200.0, 300.0), Rotation::R0, 1.0)
                .unwrap(),
            bounds: Rect::new(0.0, 0.0, 200.0, 300.0),
        })
    }

    fn response(
        document: u64,
        request: u64,
        page: u32,
        bitmap: Arc<PageBitmap>,
        complete: bool,
    ) -> TileResponse {
        TileResponse {
            document,
            request,
            page,
            previews: vec![(page, bitmap)],
            complete,
            result: Ok(vec![]),
        }
    }

    #[test]
    fn stale_results_are_ignored_and_partial_batches_keep_their_request() {
        let image = bitmap(Arc::from([]));
        let mut preview = PdfPreview::new(10, Arc::clone(&image));
        preview.pending = Some(20);
        preview
            .receive(response(9, 20, 1, Arc::clone(&image), true))
            .unwrap();
        preview
            .receive(response(10, 19, 1, Arc::clone(&image), true))
            .unwrap();
        assert!(preview.page_cache.get(&1).is_none());
        assert_eq!(preview.pending, Some(20));
        preview
            .receive(response(10, 20, 1, Arc::clone(&image), false))
            .unwrap();
        assert!(Arc::ptr_eq(&preview.page_cache.get(&1).unwrap(), &image));
        assert_eq!(preview.pending, Some(20));
        preview.receive(response(10, 20, 2, image, true)).unwrap();
        assert!(preview.page_cache.get(&2).is_some());
        assert_eq!(preview.pending, None);
    }

    #[test]
    fn page_cache_evicts_old_pages_within_the_shared_byte_budget() {
        // Shared test storage checks accounting without allocating 64 MiB.
        let image = bitmap(vec![0; 1024 * 1024].into());
        let mut preview = PdfPreview::new(10, Arc::clone(&image));
        for page in 1..64 {
            preview.pending = Some(20);
            preview
                .receive(response(10, 20, page, Arc::clone(&image), true))
                .unwrap();
        }
        assert!(Arc::ptr_eq(&preview.page_cache.get(&0).unwrap(), &image));
        assert!(preview.page_cache.contains(&1));
        preview.pending = Some(21);
        preview.receive(response(10, 21, 64, image, true)).unwrap();
        assert_eq!(preview.page_cache.used_bytes(), PAGE_CACHE_BYTES);
        assert!(preview.page_cache.contains(&0));
        assert!(!preview.page_cache.contains(&1));
        assert!(preview.page_cache.contains(&64));
    }
}
