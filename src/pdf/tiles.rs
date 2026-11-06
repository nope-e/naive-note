//! Visible-region planning and an owned-pixel LRU. No engine or window access.

use std::{collections::VecDeque, sync::Arc};

use crate::document::{CanvasViewport, PageGeometry, Point, Rect, ViewTransform};

use super::PageBitmap;

pub const TILE_EDGE: u32 = 512;
pub const TILE_OVERLAP: u32 = 2;
pub const TILE_CACHE_BYTES: usize = 128 * 1024 * 1024;
// Leave headroom for overlap, the preview and temporary buffers. Very large
// viewports use a lower level rather than thrashing a cache smaller than a frame.
const MAX_VISIBLE_TILES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    /// Half-octaves: round up so raster density is at least the display density.
    pub level: i16,
    pub x: u32,
    pub y: u32,
}

impl TileKey {
    pub fn scale(self) -> f64 {
        2.0_f64.powf(f64::from(self.level) / 2.0)
    }

    pub fn core(self) -> Rect {
        let edge = f64::from(TILE_EDGE) / self.scale();
        Rect::new(
            f64::from(self.x) * edge,
            f64::from(self.y) * edge,
            (f64::from(self.x) + 1.0) * edge,
            (f64::from(self.y) + 1.0) * edge,
        )
    }
}

pub struct PageTile {
    pub key: TileKey,
    pub bitmap: PageBitmap,
}

pub fn visible_tiles(
    preview: &PageBitmap,
    page: PageGeometry,
    view: ViewTransform,
    viewport: CanvasViewport,
) -> Vec<TileKey> {
    let density = view.scale() * viewport.pixels_per_point;
    let size = preview.geometry.paper_size();
    let preview_density =
        (f64::from(preview.width) / size.width).min(f64::from(preview.height) / size.height);
    if density <= preview_density {
        return Vec::new();
    }

    // Map all four viewport corners through PDF space, including extra view
    // rotation, CropBox offsets and UserUnit. Tiles use the original rotation.
    let logical = viewport.logical;
    let mut visible = Rect::new(
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for corner in [
        logical.min,
        Point::new(logical.max.x, logical.min.y),
        logical.max,
        Point::new(logical.min.x, logical.max.y),
    ] {
        let paper = preview
            .geometry
            .pdf_to_paper(page.paper_to_pdf(view.logical_to_paper(corner)));
        visible.min.x = visible.min.x.min(paper.x);
        visible.min.y = visible.min.y.min(paper.y);
        visible.max.x = visible.max.x.max(paper.x);
        visible.max.y = visible.max.y.max(paper.y);
    }
    visible.min.x = visible.min.x.max(0.0);
    visible.min.y = visible.min.y.max(0.0);
    visible.max.x = visible.max.x.min(size.width);
    visible.max.y = visible.max.y.min(size.height);
    if !visible.is_valid() {
        return Vec::new();
    }

    let mut level = (density.log2() * 2.0).ceil().clamp(-16.0, 16.0) as i16;
    loop {
        let scale = TileKey { level, x: 0, y: 0 }.scale();
        let grid = scale / f64::from(TILE_EDGE);
        let left = (visible.min.x * grid).floor() as u32;
        let top = (visible.min.y * grid).floor() as u32;
        let right = (visible.max.x * grid).ceil() as u32;
        let bottom = (visible.max.y * grid).ceil() as u32;
        let count = u64::from(right - left) * u64::from(bottom - top);
        if count <= MAX_VISIBLE_TILES as u64 {
            let mut keys = Vec::with_capacity(count as usize);
            for y in top..bottom {
                for x in left..right {
                    keys.push(TileKey { level, x, y });
                }
            }
            return keys;
        }
        level -= 1;
    }
}

#[derive(Default)]
pub struct TileCache {
    entries: VecDeque<Arc<PageTile>>,
    bytes: usize,
}

impl TileCache {
    pub fn get(&mut self, key: TileKey) -> Option<Arc<PageTile>> {
        let index = self.entries.iter().position(|tile| tile.key == key)?;
        let tile = self.entries.remove(index)?;
        self.entries.push_back(Arc::clone(&tile));
        Some(tile)
    }

    pub fn insert(&mut self, tile: Arc<PageTile>) {
        if self.get(tile.key).is_some() || tile.bitmap.rgba.len() > TILE_CACHE_BYTES {
            return;
        }
        self.bytes += tile.bitmap.rgba.len();
        self.entries.push_back(tile);
        while self.bytes > TILE_CACHE_BYTES {
            if let Some(removed) = self.entries.pop_front() {
                self.bytes -= removed.bitmap.rgba.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Rotation;

    fn preview() -> PageBitmap {
        PageBitmap {
            width: 1200,
            height: 1600,
            rgba: Arc::from([]),
            geometry: PageGeometry::new(Rect::new(20.0, 30.0, 320.0, 430.0), Rotation::R0, 2.0)
                .unwrap(),
            bounds: Rect::new(0.0, 0.0, 600.0, 800.0),
        }
    }

    #[test]
    fn eight_hundred_percent_uses_physical_density_and_only_visible_tiles() {
        let preview = preview();
        for dpi in [1.0, 1.5, 2.0] {
            let viewport =
                CanvasViewport::new(Rect::new(0.0, 0.0, 800.0, 600.0), dpi, (1600, 1200)).unwrap();
            let mut view = ViewTransform::default();
            view.set_zoom_at(8.0, Point::default());
            let keys = visible_tiles(&preview, preview.geometry, view, viewport);
            assert!(!keys.is_empty());
            let density = view.scale() * dpi;
            assert!(keys[0].scale() >= density);
            assert!(keys[0].scale() < density * 2.0_f64.sqrt());
            let grid = keys[0].scale() / f64::from(TILE_EDGE);
            assert_eq!(
                keys.len(),
                (800.0 / view.scale() * grid).ceil() as usize
                    * (600.0 / view.scale() * grid).ceil() as usize
            );
            // A whole 800%-rasterized page would contain hundreds of tiles.
            assert!(keys.len() < 30);
        }
    }

    #[test]
    fn fallback_is_sufficient_at_low_zoom_and_offscreen_pages_need_no_tiles() {
        let preview = preview();
        let viewport =
            CanvasViewport::new(Rect::new(0.0, 0.0, 800.0, 600.0), 1.0, (800, 600)).unwrap();
        let mut view = ViewTransform::default();
        assert!(visible_tiles(&preview, preview.geometry, view, viewport).is_empty());
        view.set_zoom_at(8.0, Point::default());
        view.pan(Point::new(2000.0, 2000.0));
        assert!(visible_tiles(&preview, preview.geometry, view, viewport).is_empty());
    }

    #[test]
    fn view_rotation_selects_the_corresponding_original_page_corner() {
        let mut preview = preview();
        let viewport =
            CanvasViewport::new(Rect::new(0.0, 0.0, 100.0, 100.0), 1.0, (100, 100)).unwrap();
        let mut view = ViewTransform::default();
        view.set_zoom_at(8.0, Point::default());
        for original in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            preview.geometry = preview.geometry.with_rotation(original);
            let size = preview.geometry.paper_size();
            for (rotation, expected) in [
                (original, Point::new(0.0, 0.0)),
                (original.next(), Point::new(0.0, size.height)),
                (original.next().next(), Point::new(size.width, size.height)),
                (original.next().next().next(), Point::new(size.width, 0.0)),
            ] {
                let page = preview.geometry.with_rotation(rotation);
                let keys = visible_tiles(&preview, page, view, viewport);
                let corner = Point::new(
                    expected.x.min(size.width - 0.01),
                    expected.y.min(size.height - 0.01),
                );
                assert!(
                    keys.iter().any(|key| key.core().contains(corner)),
                    "{original:?}, {rotation:?}"
                );
            }
        }
    }

    #[test]
    fn enormous_viewports_reduce_level_to_stay_within_cache_capacity() {
        let preview = preview();
        let viewport = CanvasViewport::new(
            Rect::new(0.0, 0.0, 100000.0, 100000.0),
            2.0,
            (200000, 200000),
        )
        .unwrap();
        let mut view = ViewTransform::default();
        view.set_zoom_at(8.0, Point::default());
        let keys = visible_tiles(&preview, preview.geometry, view, viewport);
        assert!(!keys.is_empty() && keys.len() <= MAX_VISIBLE_TILES);
        let max_edge = (TILE_EDGE + TILE_OVERLAP * 2) as usize;
        assert!(keys.len() * max_edge * max_edge * 4 < TILE_CACHE_BYTES);
    }

    #[test]
    fn cache_reuses_pixels_and_evicts_least_recently_used_tiles() {
        let mut cache = TileCache::default();
        let mut bitmap = preview();
        // Shared allocation keeps the test small; accounting is per tile payload.
        bitmap.rgba = vec![0; 1024 * 1024].into();
        let pixels = Arc::clone(&bitmap.rgba);
        for x in 0..128 {
            cache.insert(Arc::new(PageTile {
                key: TileKey { level: 8, x, y: 0 },
                bitmap: PageBitmap {
                    rgba: Arc::clone(&pixels),
                    ..preview()
                },
            }));
        }
        let oldest = TileKey {
            level: 8,
            x: 0,
            y: 0,
        };
        let reused = cache.get(oldest).unwrap();
        assert!(Arc::ptr_eq(&pixels, &reused.bitmap.rgba));
        cache.insert(Arc::new(PageTile {
            key: TileKey {
                level: 8,
                x: 128,
                y: 0,
            },
            bitmap,
        }));
        assert_eq!(cache.bytes, TILE_CACHE_BYTES);
        assert!(
            cache
                .get(TileKey {
                    level: 8,
                    x: 1,
                    y: 0
                })
                .is_none()
        );
        assert!(Arc::ptr_eq(&reused, &cache.get(oldest).unwrap()));
        cache.insert(Arc::clone(&reused));
        assert_eq!(cache.bytes, TILE_CACHE_BYTES);
    }
}
