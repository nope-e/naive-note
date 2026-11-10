use std::{collections::VecDeque, fs::File, io::Read, path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use mupdf::{
    Colorspace, Device, DisplayList, IRect, Matrix, Pixmap,
    pdf::{PdfDocument, PdfPage},
};

use crate::document::{PageGeometry, Rect, ReferenceLines, ReferencePattern, Rotation};

use super::{
    LoadedPdf, PAGE_CACHE_BYTES, PageBitmap, PageTile, TileKey,
    tiles::{TILE_EDGE, TILE_OVERLAP},
};

// Fast full-page fallback; visible tiles supply the display's actual resolution.
const MAX_BITMAP_EDGE: u32 = 1024;
const _: () = assert!(MAX_BITMAP_EDGE as usize * MAX_BITMAP_EDGE as usize * 4 <= PAGE_CACHE_BYTES);
const PREVIEW_SCALE: f32 = 1.0;
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

pub(super) struct PdfSession {
    base: Arc<[u8]>,
    document: PdfDocument,
    pages: Vec<PageGeometry>,
    prepared: VecDeque<PreparedPage>,
    reference_lines: ReferenceLines,
    edit_data: Option<super::edit_data::EditData>,
    edit_data_status: String,
    objects: Vec<Arc<crate::document::StrokeRecord>>,
    annotation_sources: Vec<Arc<crate::document::PdfAnnotationSource>>,
}

struct PreparedPage {
    index: u32,
    bounds: mupdf::Rect,
    display_list: DisplayList,
}

impl PdfSession {
    pub fn open(path: &Path) -> Result<Self> {
        // Rust handles Unicode paths. MuPDF owns a copy of these bytes, so the
        // original file is never held open or changed by this reading session.
        let file = File::open(path).context("Cannot open PDF file")?;
        ensure!(
            file.metadata()?.len() <= MAX_FILE_BYTES,
            "PDF exceeds the 256 MiB preview limit"
        );
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("Cannot read PDF file")?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE_BYTES,
            "PDF exceeds the 256 MiB preview limit"
        );
        Self::from_bytes(&bytes)
    }

    pub(super) fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let document = PdfDocument::from_bytes(bytes).context("Cannot parse PDF")?;
        ensure!(
            !document.needs_password()?,
            "Password-protected PDFs are not supported yet"
        );
        ensure!(document.page_count()? > 0, "PDF has no pages");
        // Our saved reference vectors are displayed as a live overlay instead.
        // Keep `base` intact for lossless source preservation during saves.
        let mut pages = Vec::new();
        for index in 0..document.page_count()? {
            let page = PdfPage::try_from(document.load_page(index)?)
                .with_context(|| format!("Cannot load PDF page {}", index + 1))?;
            pages.push(page_geometry(&page)?);
        }
        let (edit_data, mut edit_data_status) = super::save::load_edit_data(&document);
        if edit_data
            .as_ref()
            .and_then(|data| data.reference_lines)
            .is_some()
        {
            super::reference_lines::remove(&document)?;
        }
        let reference_lines = edit_data
            .as_ref()
            .and_then(|data| data.reference_lines)
            .map_or(
                ReferenceLines {
                    pattern: ReferencePattern::None,
                    ..ReferenceLines::default()
                },
                |layer| layer.lines,
            );
        let (objects, annotation_sources) =
            super::annotations::load_objects(&document, edit_data.as_ref())?;
        if !annotation_sources.is_empty() {
            edit_data_status.push_str(&format!(
                "; {} unverified standard annotation(s)",
                annotation_sources.len()
            ));
        }
        Ok(Self {
            base: bytes.into(),
            document,
            pages,
            prepared: VecDeque::new(),
            reference_lines,
            edit_data,
            edit_data_status,
            objects,
            annotation_sources,
        })
    }

    pub fn base(&self) -> &[u8] {
        &self.base
    }

    pub fn annotation_sources(&self) -> &[Arc<crate::document::PdfAnnotationSource>] {
        &self.annotation_sources
    }

    pub fn set_reference_lines(&mut self, lines: ReferenceLines) {
        if self.reference_lines != lines {
            self.reference_lines = lines;
            self.prepared.clear();
        }
    }

    #[cfg(test)]
    fn first_page(&self) -> Result<PdfPage> {
        PdfPage::try_from(self.document.load_page(0)?).context("Cannot load first PDF page")
    }

    pub fn preview(&mut self, path: &Path) -> Result<LoadedPdf> {
        Ok(LoadedPdf {
            path: path.to_owned(),
            page_count: self.pages.len() as u32,
            pages: self.pages.clone(),
            edit_data_status: self.edit_data_status.clone(),
            strokes: self.objects.clone(),
            reference_lines: self
                .edit_data
                .as_ref()
                .and_then(|data| data.reference_lines),
            rendered_reference_lines: self.reference_lines,
            fingerprint: crate::storage::FileFingerprint::of(&self.base),
            first_page: Arc::new(self.render_preview(0)?),
        })
    }

    fn prepare(&mut self, index: u32) -> Result<&PreparedPage> {
        ensure!(
            (index as usize) < self.pages.len(),
            "PDF page is out of range"
        );
        if let Some(position) = self.prepared.iter().position(|page| page.index == index) {
            let page = self.prepared.remove(position).unwrap();
            self.prepared.push_back(page);
        } else {
            let page = PdfPage::try_from(self.document.load_page(index as i32)?)?;
            let bounds = page.bounds()?;
            let mut display_list = DisplayList::new(bounds)?;
            let device = Device::from_display_list(&mut display_list)?;
            page.run_contents(&device, &Matrix::IDENTITY)?;
            super::reference_lines::record(&device, &page, self.reference_lines)?;
            page.run_annotations(&device, &Matrix::IDENTITY)?;
            page.run_widgets(&device, &Matrix::IDENTITY)?;
            drop(device);
            // A small LRU bounds the number of engine display lists, independent
            // of document length. Pixel caches have their own byte budgets.
            if self.prepared.len() == 4 {
                self.prepared.pop_front();
            }
            self.prepared.push_back(PreparedPage {
                index,
                bounds,
                display_list,
            });
        }
        Ok(self.prepared.back().unwrap())
    }

    pub fn render_preview(&mut self, index: u32) -> Result<PageBitmap> {
        let geometry = *self
            .pages
            .get(index as usize)
            .context("PDF page is out of range")?;
        let prepared = self.prepare(index)?;
        let bounds = prepared.bounds;
        // Leave room for MuPDF's floor/ceil rounding of the raster bounds.
        let scale =
            PREVIEW_SCALE.min((MAX_BITMAP_EDGE - 2) as f32 / bounds.width().max(bounds.height()));
        ensure!(scale.is_finite() && scale > 0.0, "Invalid PDF render scale");
        let pixmap = prepared
            .display_list
            .to_pixmap(
                &Matrix::new_scale(scale, scale),
                &Colorspace::device_rgb(),
                false,
            )
            .context("Cannot render PDF preview")?;
        let (width, height) = (pixmap.width(), pixmap.height());
        ensure!(
            width > 0 && height > 0 && width <= MAX_BITMAP_EDGE && height <= MAX_BITMAP_EDGE,
            "PDF preview exceeds bitmap limits"
        );
        let size = geometry.paper_size();
        Ok(PageBitmap {
            width,
            height,
            rgba: rgba_pixels(&pixmap)?.into(),
            geometry,
            bounds: Rect::new(0.0, 0.0, size.width, size.height),
        })
    }

    pub fn render_tile(&mut self, key: TileKey) -> Result<PageTile> {
        ensure!((-16..=16).contains(&key.level), "Invalid PDF tile level");
        let scale = key.scale();
        let geometry = *self
            .pages
            .get(key.page as usize)
            .context("PDF page is out of range")?;
        let prepared = self.prepare(key.page)?;
        let size = geometry.paper_size();
        let left = f64::from(key.x) * f64::from(TILE_EDGE);
        let top = f64::from(key.y) * f64::from(TILE_EDGE);
        ensure!(
            left < size.width * scale && top < size.height * scale,
            "PDF tile is outside the page"
        );
        let overlap = f64::from(TILE_OVERLAP);
        let x0 = (left - overlap).max(0.0);
        let y0 = (top - overlap).max(0.0);
        let x1 = (left + f64::from(TILE_EDGE) + overlap).min((size.width * scale).ceil());
        let y1 = (top + f64::from(TILE_EDGE) + overlap).min((size.height * scale).ceil());
        ensure!(
            x1 < f64::from(i32::MAX) && y1 < f64::from(i32::MAX),
            "PDF tile coordinates exceed raster limits"
        );
        let raster = IRect::new(x0 as i32, y0 as i32, x1 as i32, y1 as i32);
        let mut pixmap = Pixmap::new_with_rect(&Colorspace::device_rgb(), raster, false)
            .context("Cannot allocate PDF tile")?;
        pixmap.clear_with(255)?;
        let device = Device::from_pixmap(&pixmap).context("Cannot create PDF tile device")?;
        let matrix = Matrix::new(
            scale as f32,
            0.0,
            0.0,
            scale as f32,
            -prepared.bounds.x0 * scale as f32,
            -prepared.bounds.y0 * scale as f32,
        );
        prepared
            .display_list
            .run(
                &device,
                &matrix,
                mupdf::Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32),
            )
            .context("Cannot render PDF tile")?;
        // Closing the draw device completes transparency groups before reading.
        drop(device);
        Ok(PageTile {
            key,
            bitmap: PageBitmap {
                width: pixmap.width(),
                height: pixmap.height(),
                rgba: rgba_pixels(&pixmap)?.into(),
                geometry,
                bounds: Rect::new(x0 / scale, y0 / scale, x1 / scale, y1 / scale),
            },
        })
    }
}

fn rgba_pixels(pixmap: &Pixmap) -> Result<Vec<u8>> {
    ensure!(
        pixmap.n() == 3 && !pixmap.alpha(),
        "Unexpected MuPDF pixel format"
    );
    let (width, height) = (pixmap.width(), pixmap.height());
    let stride = usize::try_from(pixmap.stride()).context("Invalid MuPDF row stride")?;
    let row_bytes = width as usize * 3;
    ensure!(
        stride >= row_bytes && pixmap.samples().len() >= stride * height as usize,
        "Incomplete MuPDF pixel buffer"
    );
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in pixmap.samples().chunks_exact(stride).take(height as usize) {
        for rgb in row[..row_bytes].as_chunks::<3>().0 {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Ok(rgba)
}

pub(super) fn page_geometry(page: &PdfPage) -> Result<PageGeometry> {
    // Derive the effective CropBox from MuPDF itself: it intersects MediaBox,
    // honors inherited page attributes and applies its malformed-box fallback.
    // PdfPage::crop_box() in 0.8.0 mixes rotated bounds with unrotated offsets.
    let ctm = page.ctm()?;
    let inverse = ctm.invert().context("Non-invertible PDF page transform")?;
    let crop = page.bounds()?.transform(&inverse);
    let user_unit = f64::from(ctm.a).hypot(f64::from(ctm.b));
    let degrees = ((page.rotation()?.rem_euclid(360) + 45) / 90 * 90) % 360;
    let rotation = match degrees {
        90 => Rotation::R90,
        180 => Rotation::R180,
        270 => Rotation::R270,
        _ => Rotation::R0,
    };
    PageGeometry::new(
        Rect::new(
            f64::from(crop.x0),
            f64::from(crop.y0),
            f64::from(crop.x1),
            f64::from(crop.y1),
        ),
        rotation,
        user_unit,
    )
    .context("Invalid PDF page geometry")
}

#[cfg(test)]
mod tests;
