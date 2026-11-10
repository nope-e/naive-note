//! Replace only our own vector streams; preserve the source page content.

use std::fmt::Write as _;

use anyhow::Result;
use mupdf::{
    Buffer,
    pdf::{PdfDocument, PdfObject},
};

use crate::document::{
    Point, Rect, ReferenceLayer, ReferenceLines, ReferenceMark, ReferencePattern,
};

/// Record references between page content and annotations. This also covers
/// third-party Ink annotations which have no Naive Note editing metadata.
pub(super) fn record(
    device: &mupdf::Device,
    page: &mupdf::pdf::PdfPage,
    lines: ReferenceLines,
) -> Result<()> {
    use mupdf::{ColorParams, Colorspace, LineCap, LineJoin, Path, StrokeState};
    if lines.pattern == ReferencePattern::None {
        return Ok(());
    }
    let geometry = super::backend::page_geometry(page)?;
    let crop = geometry.crop_box();
    let size = geometry.unrotated_size();
    let unit = geometry.user_unit();
    let project = |p: Point| {
        (
            (crop.min.x + p.x / unit) as f32,
            (crop.max.y - p.y / unit) as f32,
        )
    };
    let mut marks = Vec::new();
    lines.visit(Rect::new(0.0, 0.0, size.width, size.height), |mark| {
        marks.push(mark)
    })?;
    let mut paths = Path::new()?;
    let mut dots = Path::new()?;
    for mark in marks {
        match mark {
            ReferenceMark::Line(a, b) => {
                let (a, b) = (project(a), project(b));
                paths.move_to(a.0, a.1)?;
                paths.line_to(b.0, b.1)?;
            }
            ReferenceMark::Dot(p) => {
                let (x, y) = project(p);
                let radius = (0.7 / unit) as f32;
                dots.rect(x - radius, y - radius, x + radius, y + radius)?;
            }
        }
    }
    let ctm = page.ctm()?;
    let mut clip = Path::new()?;
    clip.rect(
        crop.min.x as f32,
        crop.min.y as f32,
        crop.max.x as f32,
        crop.max.y as f32,
    )?;
    device.clip_path(&clip, false, &ctm)?;
    let color = lines.color_rgb.map(|c| f32::from(c) / 255.0);
    let stroke = StrokeState::new(
        LineCap::Butt,
        LineCap::Butt,
        LineCap::Butt,
        LineJoin::Miter,
        (0.45 / unit) as f32,
        10.0,
        0.0,
        &[],
    )?;
    device.stroke_path(
        &paths,
        &stroke,
        &ctm,
        &Colorspace::device_rgb(),
        &color,
        1.0,
        ColorParams::default(),
    )?;
    device.fill_path(
        &dots,
        false,
        &ctm,
        &Colorspace::device_rgb(),
        &color,
        1.0,
        ColorParams::default(),
    )?;
    device.pop_clip()?;
    Ok(())
}

const MARKER: &str = "NaiveNoteReferenceLines";

pub(super) fn remove(document: &PdfDocument) -> Result<()> {
    for index in 0..document.page_count()? {
        let mut page = document.load_pdf_page(index)?.object();
        let Some(contents) = page.get_dict("Contents")? else {
            continue;
        };
        let mut kept = document.new_array()?;
        let mut removed = false;
        if contents.is_array()? {
            for item in contents.array_iter()? {
                let item = item?;
                if item.get_dict(MARKER)?.is_none() {
                    kept.array_push(item)?;
                } else {
                    removed = true;
                }
            }
        } else if contents.get_dict(MARKER)?.is_some() {
            removed = true;
        }
        if removed {
            page.dict_put("Contents", kept)?;
        }
    }
    Ok(())
}

fn stream(document: &mut PdfDocument, content: &str) -> Result<PdfObject> {
    let mut dict = document.new_dict()?;
    dict.dict_put(MARKER, PdfObject::new_bool(true))?;
    Ok(document.add_stream(&Buffer::from_bytes(content.as_bytes())?, Some(&dict), false)?)
}

pub(super) fn write(document: &mut PdfDocument, layer: ReferenceLayer) -> Result<()> {
    layer.lines.validate()?;
    remove(document)?;
    if !layer.embed_in_pdf || layer.lines.pattern == ReferencePattern::None {
        return Ok(());
    }
    for index in 0..document.page_count()? {
        let page = document.load_pdf_page(index)?;
        let geometry = super::backend::page_geometry(&page)?;
        let crop = geometry.crop_box();
        let size = geometry.unrotated_size();
        let unit = geometry.user_unit();
        let project = |p: Point| Point::new(crop.min.x + p.x / unit, crop.max.y - p.y / unit);
        let [r, g, b] = layer.lines.color_rgb.map(|v| f64::from(v) / 255.0);
        // Balance a preceding save around source streams so their graphics state
        // cannot change our coordinates, color, clipping or line style.
        let mut content = format!(
            "Q\nq\n{:.6} {:.6} {:.6} {:.6} re W n\n{r:.8} {g:.8} {b:.8} RG\n{r:.8} {g:.8} {b:.8} rg\n{:.8} w\n0 J 0 j [] 0 d\n",
            crop.min.x,
            crop.min.y,
            crop.size().width,
            crop.size().height,
            0.45 / unit
        );
        layer.lines.visit(
            Rect::new(0.0, 0.0, size.width, size.height),
            |mark| match mark {
                ReferenceMark::Line(a, b) => {
                    let (a, b) = (project(a), project(b));
                    writeln!(content, "{:.6} {:.6} m {:.6} {:.6} l S", a.x, a.y, b.x, b.y).unwrap();
                }
                ReferenceMark::Dot(p) => {
                    let p = project(p);
                    let edge = 1.4 / unit;
                    writeln!(
                        content,
                        "{:.6} {:.6} {edge:.6} {edge:.6} re f",
                        p.x - edge / 2.0,
                        p.y - edge / 2.0
                    )
                    .unwrap();
                }
            },
        )?;
        content.push_str("Q\n");
        let mut object = page.object();
        let mut contents = document.new_array()?;
        contents.array_push(stream(document, "q\n")?)?;
        if let Some(original) = object.get_dict("Contents")? {
            if original.is_array()? {
                for item in original.array_iter()? {
                    contents.array_push(item?)?;
                }
            } else {
                contents.array_push(original)?;
            }
        }
        contents.array_push(stream(document, &content)?)?;
        object.dict_put("Contents", contents)?;
    }
    Ok(())
}
