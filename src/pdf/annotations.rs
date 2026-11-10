//! Import current standard annotation appearances, independent of private metadata.

mod saving;

pub(super) use saving::{apply_edits, order_objects, verify_saved};

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, ensure};
use mupdf::{
    Device, DisplayList, Matrix,
    pdf::{PdfDocument, PdfObject, PdfPage},
};
use uuid::Uuid;

use crate::document::{
    InkTransform, PdfAnnotationSource, PenStyle, ProcessingSnapshot, Rect, StrokeRecord,
};

use super::{backend::page_geometry, edit_data::EditData};

type LoadedObjects = (Vec<Arc<StrokeRecord>>, Vec<Arc<PdfAnnotationSource>>);

/// Preserve per-page annotation stacking while separating the editable objects
/// from the immutable page preview. Verification is a label, not an edit gate.
pub(super) fn load_objects(
    document: &PdfDocument,
    data: Option<&EditData>,
) -> Result<LoadedObjects> {
    let mut native: HashMap<_, _> = data
        .into_iter()
        .flat_map(|data| &data.strokes)
        .map(|saved| (saved.stroke.id.to_string(), Arc::new(saved.stroke.clone())))
        .collect();
    let mut objects = Vec::new();
    let mut sources = Vec::new();
    for index in 0..document.page_count()? {
        let mut page = document.load_pdf_page(index)?;
        let annotations = page.annotations().collect::<Vec<_>>();
        let mut removed = Vec::new();
        for annotation in annotations {
            let object = annotation.object();
            let name = object
                .get_dict("NM")?
                .and_then(|name| name.as_string().ok());
            if let Some(stroke) = name.and_then(|name| native.remove(&name)) {
                objects.push(stroke);
                removed.push(annotation);
                continue;
            }
            let subtype = object
                .get_dict("Subtype")?
                .map(|name| name.as_name())
                .transpose()?;
            if matches!(subtype.as_deref(), Some(b"Popup" | b"Widget" | b"Link")) {
                continue;
            }
            match import_annotation(document, &page, &object, annotation.xref()?, index as u32) {
                Ok(source) => {
                    let source = Arc::new(source);
                    objects.push(Arc::new(StrokeRecord {
                        id: Uuid::new_v4(),
                        page_index: index as u32,
                        edit_data_verified: false,
                        pdf_annotation: Some(Arc::clone(&source)),
                        raw_samples: Vec::new(),
                        edit_transform: InkTransform::default(),
                        modeled_samples: Vec::new(),
                        outline: Vec::new(),
                        style: PenStyle::default(),
                        processing: ProcessingSnapshot {
                            algorithm: "standard-pdf-annotation".to_owned(),
                            version: 1,
                            coordinate_unit: "paper-pt-top-left".to_owned(),
                            modeler: None,
                        },
                    }));
                    sources.push(source);
                    removed.push(annotation);
                }
                Err(error) => log::warn!("Cannot import PDF annotation: {error:#}"),
            }
        }
        for annotation in removed {
            page.delete_annotation(annotation)?;
        }
    }
    Ok((objects, sources))
}

fn import_annotation(
    document: &PdfDocument,
    page: &PdfPage,
    object: &PdfObject,
    xref: i32,
    page_index: u32,
) -> Result<PdfAnnotationSource> {
    ensure!(xref > 0, "Direct annotation objects are unsupported");
    // A small standalone page preserves AP resources (including fonts/images)
    // while avoiding links back to the source page/document through /P or replies.
    let mut standalone = PdfDocument::new();
    let geometry = page_geometry(page)?;
    let size = geometry.unrotated_size();
    let mut target =
        standalone.new_page_at(0, mupdf::Size::new(size.width as f32, size.height as f32))?;
    for key in ["MediaBox", "CropBox"] {
        target.object().dict_put(
            key,
            array(
                &standalone,
                &[
                    geometry.crop_box().min.x,
                    geometry.crop_box().min.y,
                    geometry.crop_box().max.x,
                    geometry.crop_box().max.y,
                ],
            )?,
        )?;
    }
    target.set_rotation(geometry.rotation().degrees())?;
    target.object().dict_put(
        "UserUnit",
        PdfObject::new_real(geometry.user_unit() as f32)?,
    )?;
    let mut copy = document.new_dict()?;
    for entry in object.dict_iter()? {
        let (key, value) = entry?;
        let key = String::from_utf8(key.as_name()?)?;
        if !matches!(
            key.as_str(),
            "P" | "Popup" | "Parent" | "IRT" | "FS" | "A" | "AA"
        ) {
            copy.dict_put(key.as_str(), value)?;
        }
    }
    let copied = standalone.graft_object(&copy)?;
    let copied = standalone.add_object(&copied)?;
    let mut annotations = standalone.new_array()?;
    annotations.array_push(copied)?;
    target.object().dict_put("Annots", annotations)?;
    // Reload to let MuPDF generate missing standard appearances and attach the
    // copied dictionary to its new page. This does not change the original PDF.
    let mut bytes = Vec::new();
    standalone.write_to(&mut bytes)?;
    let standalone = PdfDocument::from_bytes(&bytes)?;
    let page = standalone.load_pdf_page(0)?;
    let annotation = page
        .annotations()
        .next()
        .context("Missing standalone annotation")?;
    let rect = annotation.bounds()?;
    let bounds = Rect::new(
        rect.x0 as f64,
        rect.y0 as f64,
        rect.x1 as f64,
        rect.y1 as f64,
    );
    ensure!(bounds.is_valid(), "Invalid annotation bounds");
    let mut list = DisplayList::new(rect)?;
    let device = Device::from_display_list(&mut list)?;
    page.run_annotations(&device, &Matrix::IDENTITY)?;
    drop(device);
    let svg = list.to_svg(&Matrix::new_translate(-rect.x0, -rect.y0))?;
    let multiply = annotation.r#type()? == mupdf::pdf::PdfAnnotationType::Highlight
        || uses_multiply(&annotation.object())?
        || svg.contains("mix-blend-mode:multiply")
        || svg.contains("mix-blend-mode: multiply");
    let mut appearance_pdf = Vec::new();
    standalone.write_to(&mut appearance_pdf)?;
    Ok(PdfAnnotationSource {
        xref,
        page_index,
        bounds,
        svg: svg.into(),
        appearance_pdf: appearance_pdf.into(),
        multiply,
    })
}

fn uses_multiply(object: &PdfObject) -> Result<bool> {
    let mut pending = vec![object.clone()];
    let mut seen = std::collections::HashSet::new();
    while let Some(object) = pending.pop() {
        if !object.is_dict()? {
            continue;
        }
        if object.is_indirect()? && !seen.insert(object.as_indirect()?) {
            continue;
        }
        if object
            .get_dict("BM")?
            .is_some_and(|mode| mode.as_name().is_ok_and(|name| name == b"Multiply"))
        {
            return Ok(true);
        }
        for key in ["AP", "Resources", "XObject", "ExtGState"] {
            if let Some(children) = object.get_dict(key)? {
                for child in children.dict_iter()? {
                    pending.push(child?.1);
                }
            }
        }
    }
    Ok(false)
}

fn array(document: &PdfDocument, values: &[f64]) -> Result<PdfObject> {
    let mut array = document.new_array()?;
    for value in values {
        ensure!(value.is_finite(), "Invalid annotation coordinate");
        array.array_push(PdfObject::new_real(*value as f32)?)?;
    }
    Ok(array)
}

#[cfg(test)]
mod tests;
