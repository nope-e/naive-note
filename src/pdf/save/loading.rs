//! Reconcile editable records with current annotations; saved output is checked strictly.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, ensure};
use mupdf::pdf::{PdfDocument, PdfObject};

use crate::pdf::edit_data::{EditData, SavedStroke};

use super::{
    attachment_names, fingerprint, is_name, page_geometry, required, validate_stroke,
    verify_geometry,
};

/// Deleted or changed annotations do not invalidate unrelated native records or
/// document settings. The PDF itself remains authoritative for skipped records.
pub(in crate::pdf) fn load_edit_data(document: &PdfDocument) -> (Option<EditData>, String) {
    match read_verified_data(document, true) {
        Ok(Some((data, skipped))) => {
            let mut status = format!(
                "Edit data v{}: {} verified stroke(s)",
                data.format_version,
                data.strokes.len()
            );
            if skipped > 0 {
                status.push_str(&format!("; {skipped} deleted or changed stroke(s) skipped"));
            }
            (Some(data), status)
        }
        Ok(None) => (None, "No Naive Note edit data".to_owned()),
        Err(error) => {
            log::warn!("Ignoring edit metadata: {error:#}");
            (None, format!("Edit data ignored: {error:#}"))
        }
    }
}

/// Read-back validation must reject every mismatch in our own saved snapshot.
pub(in crate::pdf) fn read_edit_data(document: &PdfDocument) -> Result<Option<EditData>> {
    Ok(read_verified_data(document, false)?.map(|(data, _)| data))
}

// None marks a duplicated name, including duplicates on different pages. Such
// objects must never be removed or reconstructed from a private record.
type AnnotationIndex = HashMap<String, Option<(u32, PdfObject)>>;

fn read_verified_data(
    document: &PdfDocument,
    allow_external_changes: bool,
) -> Result<Option<(EditData, usize)>> {
    let Some(mut data) = read_attachment(document)? else {
        return Ok(None);
    };
    let mut ids = HashSet::new();
    for saved in &data.strokes {
        ensure!(
            ids.insert(saved.stroke.id.to_string()),
            "Duplicate stroke UUID in attachment"
        );
    }
    let mut annotations = AnnotationIndex::new();
    if !ids.is_empty() {
        for index in 0..document.page_count()? {
            for annotation in document.load_pdf_page(index)?.annotations() {
                let object = annotation.object();
                if let Some(name) = object.get_dict("NM")?
                    && let Ok(name) = name.as_string()
                    && ids.contains(&name)
                {
                    annotations
                        .entry(name)
                        .and_modify(|entry| *entry = None)
                        .or_insert(Some((index as u32, object)));
                }
            }
        }
    }
    let mut skipped = 0;
    for saved in std::mem::take(&mut data.strokes) {
        if let Err(error) = verify_stroke(document, &annotations, &saved) {
            if !allow_external_changes {
                return Err(error);
            }
            log::warn!("Skipping edit record {}: {error:#}", saved.stroke.id);
            skipped += 1;
        } else {
            data.strokes.push(saved);
        }
    }
    Ok(Some((data, skipped)))
}

fn verify_stroke(
    document: &PdfDocument,
    annotations: &AnnotationIndex,
    saved: &SavedStroke,
) -> Result<()> {
    let (page_index, object) = annotations
        .get(&saved.stroke.id.to_string())
        .and_then(Option::as_ref)
        .with_context(|| format!("Missing or duplicated annotation {}", saved.stroke.id))?;
    ensure!(
        *page_index == saved.stroke.page_index,
        "Annotation page changed"
    );
    let page = document.load_pdf_page(i32::try_from(*page_index)?)?;
    let geometry = page_geometry(&page)?;
    validate_stroke(&saved.stroke, geometry)?;
    ensure!(
        is_name(object.get_dict("Subtype")?, b"Ink")?,
        "Annotation is no longer Ink"
    );
    ensure!(
        fingerprint(object)? == saved.annotation_sha256,
        "Annotation fingerprint mismatch for {}",
        saved.stroke.id
    );
    verify_geometry(object, &saved.stroke, geometry, saved.appearance_version)
}

fn read_attachment(document: &PdfDocument) -> Result<Option<EditData>> {
    let names: Vec<_> = attachment_names(document)?
        .into_iter()
        .filter(|n| n.starts_with("naive-note.") && n.ends_with(".cbor.zst"))
        .collect();
    ensure!(names.len() <= 1, "Multiple Naive Note edit attachments");
    let Some(name) = names.first() else {
        return Ok(None);
    };
    // Work around the binding's flat-only reader without modifying the document.
    let flat = document.load_name_tree(PdfObject::new_name("EmbeddedFiles")?)?;
    let spec = required(&flat, name)?;
    let ef = required(&spec, "EF")?;
    let stream = ef
        .get_dict("UF")?
        .or(ef.get_dict("F")?)
        .context("Missing embedded file stream")?;
    if let Some(length) = stream.get_dict("Length")? {
        ensure!(length.as_int()? >= 0, "Invalid edit attachment length");
    }
    let data = EditData::decode(&stream.read_stream()?)?;
    ensure!(
        &data.filename() == name,
        "Attachment document UUID does not match its name"
    );
    Ok(Some(data))
}
