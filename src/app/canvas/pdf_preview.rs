//! One render queue and bounded pixel caches for all visible PDF pages.

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    pdf::{PageBitmap, PdfWorker, TileCache, TileKey, TileResponse, visible_tiles},
    render::CanvasScene,
};

const REFINE_DELAY: Duration = Duration::from_millis(80);
const PREVIEW_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_DETAIL_TILES: usize = 96;

pub(super) struct PdfPreview {
    document: u64,
    cache: TileCache,
    previews: VecDeque<(u32, Arc<PageBitmap>)>,
    preview_bytes: usize,
    visible: Vec<TileKey>,
    pages: Vec<u32>,
    pending: Option<u64>,
    due: Instant,
    failed: HashSet<u32>,
}

impl PdfPreview {
    pub fn new(document: u64, first_page: Arc<PageBitmap>) -> Self {
        let preview_bytes = first_page.rgba.len();
        Self {
            document,
            cache: TileCache::default(),
            previews: VecDeque::from([(0, first_page)]),
            preview_bytes,
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
            if let Some(index) = self.previews.iter().position(|(cached, _)| *cached == page) {
                self.preview_bytes -= self.previews.remove(index).unwrap().1.rgba.len();
            }
            self.preview_bytes += bitmap.rgba.len();
            self.previews.push_back((page, bitmap));
            while self.preview_bytes > PREVIEW_CACHE_BYTES {
                if let Some((_, removed)) = self.previews.pop_front() {
                    self.preview_bytes -= removed.rgba.len();
                }
            }
        }
        match response.result {
            Ok(tiles) => {
                for tile in tiles {
                    self.cache.insert(tile);
                }
                Ok(())
            }
            Err(error) => {
                self.failed.insert(response.page);
                Err(error)
            }
        }
    }

    fn bitmap(&mut self, page: u32) -> Option<Arc<PageBitmap>> {
        let index = self.previews.iter().position(|(index, _)| *index == page)?;
        let entry = self.previews.remove(index)?;
        let bitmap = Arc::clone(&entry.1);
        self.previews.push_back(entry);
        Some(bitmap)
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
            page.bitmap = self.bitmap(page.index);
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
            .filter(|page| {
                !self.failed.contains(page)
                    && !self.previews.iter().any(|(cached, _)| cached == page)
            })
            .take(4)
            .collect();
        let mut missing = Vec::new();
        for key in &self.visible {
            if let Some(tile) = self.cache.get(*key) {
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
        assert!(preview.bitmap(1).is_none());
        assert_eq!(preview.pending, Some(20));
        preview
            .receive(response(10, 20, 1, Arc::clone(&image), false))
            .unwrap();
        assert!(Arc::ptr_eq(&preview.bitmap(1).unwrap(), &image));
        assert_eq!(preview.pending, Some(20));
        preview.receive(response(10, 20, 2, image, true)).unwrap();
        assert!(preview.bitmap(2).is_some());
        assert_eq!(preview.pending, None);
    }

    #[test]
    fn preview_cache_evicts_old_pages_within_the_shared_byte_budget() {
        // Shared test storage checks accounting without allocating 64 MiB.
        let image = bitmap(vec![0; 1024 * 1024].into());
        let mut preview = PdfPreview::new(10, Arc::clone(&image));
        for page in 1..64 {
            preview.pending = Some(20);
            preview
                .receive(response(10, 20, page, Arc::clone(&image), true))
                .unwrap();
        }
        preview.bitmap(0).unwrap();
        preview.pending = Some(21);
        preview.receive(response(10, 21, 64, image, true)).unwrap();
        assert_eq!(preview.preview_bytes, PREVIEW_CACHE_BYTES);
        assert!(preview.bitmap(0).is_some());
        assert!(preview.bitmap(1).is_none());
        assert!(preview.bitmap(64).is_some());
    }
}
