use std::{fs::File, io::Read, path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use mupdf::{
    Colorspace, Matrix,
    pdf::{PdfDocument, PdfPage},
};

use crate::document::{PageGeometry, Rect, Rotation};

use super::{LoadedPdf, PageBitmap};

// First-page preview only. A later tile cache will replace this fixed resolution.
const MAX_BITMAP_EDGE: u32 = 2048;
const PREVIEW_SCALE: f32 = 2.0;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

pub(super) struct PdfSession {
    document: PdfDocument,
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

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let document = PdfDocument::from_bytes(bytes).context("Cannot parse PDF")?;
        ensure!(
            !document.needs_password()?,
            "Password-protected PDFs are not supported yet"
        );
        ensure!(document.page_count()? > 0, "PDF has no pages");
        Ok(Self { document })
    }

    fn first_page(&self) -> Result<PdfPage> {
        PdfPage::try_from(self.document.load_page(0)?).context("Cannot load first PDF page")
    }

    pub fn preview(&self, path: &Path) -> Result<LoadedPdf> {
        let page = self.first_page()?;
        let geometry = page_geometry(&page)?;
        let bounds = page.bounds()?;
        ensure!(
            bounds.width().is_finite()
                && bounds.height().is_finite()
                && bounds.width() > 0.0
                && bounds.height() > 0.0,
            "Invalid PDF page bounds"
        );
        // Leave room for MuPDF's floor/ceil rounding of the raster bounds.
        let scale =
            PREVIEW_SCALE.min((MAX_BITMAP_EDGE - 2) as f32 / bounds.width().max(bounds.height()));
        ensure!(scale.is_finite() && scale > 0.0, "Invalid PDF render scale");
        let pixmap = page
            .to_pixmap(
                &Matrix::new_scale(scale, scale),
                &Colorspace::device_rgb(),
                false,
                true,
            )
            .context("Cannot render first PDF page")?;
        let (width, height) = (pixmap.width(), pixmap.height());
        ensure!(
            width > 0 && height > 0 && width <= MAX_BITMAP_EDGE && height <= MAX_BITMAP_EDGE,
            "PDF preview exceeds bitmap limits"
        );
        ensure!(
            pixmap.n() == 3 && !pixmap.alpha(),
            "Unexpected MuPDF pixel format"
        );
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
        Ok(LoadedPdf {
            path: path.to_owned(),
            page_count: u32::try_from(self.document.page_count()?)?,
            first_page: Arc::new(PageBitmap {
                width,
                height,
                rgba,
                geometry,
            }),
        })
    }
}

fn page_geometry(page: &PdfPage) -> Result<PageGeometry> {
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
