//! Standard Ink annotations, pressure appearances and verified full-copy serialization.

use std::{collections::HashSet, fmt::Write as _, fs, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use mupdf::{
    Buffer,
    color::AnnotationColor,
    pdf::{AnnotationFlags, EmbeddedFileOptions, PdfDocument, PdfObject, Permission},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::document::{PageGeometry, PathCommand, Point, StrokeRecord};

use super::{
    backend::page_geometry,
    edit_data::{EditData, FORMAT_VERSION, MAX_EDIT_BYTES, SavedStroke},
};

pub(super) fn read_pdf(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).context("Cannot open PDF file")?;
    const MAX: u64 = 256 * 1024 * 1024;
    ensure!(
        file.metadata()?.len() <= MAX,
        "PDF exceeds the 256 MiB limit"
    );
    let mut bytes = Vec::new();
    file.take(MAX + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX, "PDF exceeds the 256 MiB limit");
    Ok(bytes)
}

fn ensure_writable(document: &PdfDocument) -> Result<()> {
    ensure!(
        !document.needs_password()?,
        "Password-protected PDF is read-only"
    );
    ensure!(
        document.trailer()?.get_dict("Encrypt")?.is_none(),
        "Encrypted PDF is read-only"
    );
    ensure!(
        document
            .permissions()
            .contains(Permission::ANNOTATE | Permission::MODIFY),
        "PDF does not allow annotation and attachment changes"
    );
    ensure!(
        document.catalog()?.get_dict("Perms")?.is_none(),
        "Certified PDF is read-only"
    );
    // A signed field can exist without /Perms; conservatively reject signature
    // dictionaries anywhere in the xref, including orphaned signature objects.
    for n in 1..document.count_objects()? {
        if let Some(object) = document.xref_object(n as i32)? {
            if !object.is_dict()? {
                continue;
            }
            ensure!(
                !is_name(object.get_dict("Type")?, b"Sig")?
                    && object.get_dict("ByteRange")?.is_none(),
                "Signed PDF is read-only"
            );
        }
    }
    Ok(())
}

fn add_stroke(document: &mut PdfDocument, stroke: &StrokeRecord) -> Result<SavedStroke> {
    let mut page = document.load_pdf_page(i32::try_from(stroke.page_index)?)?;
    let geometry = page_geometry(&page)?;
    validate_stroke(stroke, geometry)?;
    let centerline: Vec<_> = stroke
        .modeled_samples
        .iter()
        .map(|s| mupdf::Point::new(s.position.x as f32, s.position.y as f32))
        .collect();
    let mut annotation = page.add_ink_annotation([centerline])?;
    annotation.set_flags(AnnotationFlags::IS_PRINT)?;
    let [r, g, b] = stroke.style.color_rgb.map(|c| f32::from(c) / 255.0);
    annotation.set_color(AnnotationColor::Rgb {
        red: r,
        green: g,
        blue: b,
    })?;
    annotation.set_border_width((stroke.style.max_width_pt / geometry.user_unit()) as f32)?;
    let mut object = annotation.object();
    object.dict_put("NM", PdfObject::new_string(&stroke.id.to_string())?)?;
    object.dict_put(
        "Contents",
        PdfObject::new_string("Naive Note pressure stroke")?,
    )?;
    // First clear MuPDF's pending appearance regeneration, then install the
    // final custom Form XObject. No semantic setter may run after this point.
    annotation.update()?;
    let (rect, contents) = appearance(stroke, geometry)?;
    object.dict_put("Rect", numbers(document, &rect)?)?;
    let mut form = document.new_dict()?;
    form.dict_put("Type", PdfObject::new_name("XObject")?)?;
    form.dict_put("Subtype", PdfObject::new_name("Form")?)?;
    form.dict_put("FormType", PdfObject::new_int(1)?)?;
    form.dict_put("BBox", numbers(document, &rect)?)?;
    form.dict_put("Matrix", numbers(document, &[1., 0., 0., 1., 0., 0.])?)?;
    form.dict_put("Resources", document.new_dict()?)?;
    let stream = document.add_stream(
        &Buffer::from_bytes(contents.as_bytes())?,
        Some(&form),
        false,
    )?;
    let mut ap = document.new_dict()?;
    ap.dict_put("N", stream)?;
    object.dict_put("AP", ap)?;
    // This deliberately exercises the update path that could overwrite /AP.
    page.update()?;
    ensure!(
        normal_appearance(&object)?.read_stream()? == contents.as_bytes(),
        "MuPDF replaced the pressure appearance during page update"
    );
    Ok(SavedStroke {
        stroke: stroke.clone(),
        annotation_sha256: fingerprint(&object)?,
    })
}

/// Serialize only the current session strokes over the original immutable PDF.
/// Invalid private metadata is replaced; all actual PDF annotations are retained.
pub(super) fn serialize(
    base: &[u8],
    strokes: &[std::sync::Arc<StrokeRecord>],
    document_id: Uuid,
) -> Result<(Vec<u8>, EditData)> {
    let mut document = PdfDocument::from_bytes(base)?;
    ensure_writable(&document)?;
    let mut data = read_edit_data(&document)
        .unwrap_or_else(|error| {
            log::warn!("Replacing ignored edit metadata during save: {error:#}");
            None
        })
        .unwrap_or(EditData {
            format_version: FORMAT_VERSION,
            document_id,
            strokes: Vec::new(),
        });
    let mut ids: HashSet<_> = data.strokes.iter().map(|s| s.stroke.id).collect();
    // Also reserve UUIDs of standard annotations whose metadata was discarded.
    for index in 0..document.page_count()? {
        for annotation in document.load_pdf_page(index)?.annotations() {
            if let Some(name) = annotation.object().get_dict("NM")?
                && let Ok(id) = Uuid::parse_str(&name.as_string()?)
            {
                ids.insert(id);
            }
        }
    }
    let mut counts = vec![0; document.page_count()? as usize];
    for index in 0..document.page_count()? {
        counts[index as usize] = document.load_pdf_page(index)?.annotations().count();
    }
    for stroke in strokes {
        ensure!(ids.insert(stroke.id), "Duplicate stroke UUID");
        let index = stroke.page_index as usize;
        ensure!(index < counts.len(), "Stroke page is out of range");
        data.strokes.push(add_stroke(&mut document, stroke)?);
        counts[index] += 1;
    }
    flatten_embedded_files(&document)?;
    remove_edit_attachments(&document)?;
    let filename = data.filename();
    document.add_embedded_file(
        &filename,
        &data.encode()?,
        EmbeddedFileOptions {
            mime_type: Some("application/zstd"),
            ..EmbeddedFileOptions::new(&filename)
        },
    )?;
    sort_embedded_files(&document)?;
    let mut bytes = Vec::new();
    document
        .write_to(&mut bytes)
        .context("Serialize PDF save snapshot")?;
    let reopened = PdfDocument::from_bytes(&bytes)?;
    ensure!(
        reopened.page_count()? == document.page_count()?,
        "Saved page count changed"
    );
    for index in 0..document.page_count()? {
        let before = document.load_pdf_page(index)?;
        let after = reopened.load_pdf_page(index)?;
        ensure!(
            page_geometry(&before)? == page_geometry(&after)?,
            "Saved page geometry changed"
        );
        ensure!(
            after.annotations().count() == counts[index as usize],
            "Saved annotation count changed"
        );
    }
    verify_data(&bytes, &data)?;
    Ok((bytes, data))
}

pub(super) fn verify_data(bytes: &[u8], expected: &EditData) -> Result<()> {
    let reopened = PdfDocument::from_bytes(bytes)?;
    ensure!(
        read_edit_data(&reopened)? == Some(expected.clone()),
        "Saved edit data does not match"
    );
    Ok(())
}

fn remove_edit_attachments(document: &PdfDocument) -> Result<()> {
    let Some(names) = document.catalog()?.get_dict("Names")? else {
        return Ok(());
    };
    let Some(mut tree) = names.get_dict("EmbeddedFiles")? else {
        return Ok(());
    };
    let array = required(&tree, "Names")?;
    let mut kept = document.new_array()?;
    for index in (0..array.len()?).step_by(2) {
        let name = array
            .get_array(index as i32)?
            .context("Missing attachment name")?;
        let text = name.as_string()?;
        if !(text.starts_with("naive-note.") && text.ends_with(".cbor.zst")) {
            kept.array_push(name)?;
            kept.array_push(
                array
                    .get_array(index as i32 + 1)?
                    .context("Missing attachment filespec")?,
            )?;
        }
    }
    tree.dict_put("Names", kept)?;
    Ok(())
}

/// Persist the draft's squared paper as ordinary PDF page content.
pub(super) fn draft_pdf(geometry: PageGeometry) -> Result<Vec<u8>> {
    let mut document = PdfDocument::new();
    let crop = geometry.crop_box();
    let page = document.new_page_at(0, mupdf::Size::new(crop.max.x as f32, crop.max.y as f32))?;
    let mut object = page.object();
    let rect = [crop.min.x, crop.min.y, crop.max.x, crop.max.y];
    object.dict_put("MediaBox", numbers(&document, &rect)?)?;
    object.dict_put("CropBox", numbers(&document, &rect)?)?;
    object.dict_put("Rotate", PdfObject::new_int(geometry.rotation().degrees())?)?;
    object.dict_put(
        "UserUnit",
        PdfObject::new_real(geometry.user_unit() as f32)?,
    )?;
    let size = geometry.unrotated_size();
    let unit = geometry.user_unit();
    let mut content = format!(
        "q\n0.8627451 0.9215686 0.9921569 RG\n{:.8} w\n",
        0.45 / unit
    );
    for i in 1..=(size.width / 18.0) as u32 {
        let x = crop.min.x + f64::from(i) * 18.0 / unit;
        writeln!(
            content,
            "{x:.6} {:.6} m {x:.6} {:.6} l",
            crop.min.y, crop.max.y
        )?;
    }
    for i in 1..=(size.height / 18.0) as u32 {
        let y = crop.max.y - f64::from(i) * 18.0 / unit;
        writeln!(
            content,
            "{:.6} {y:.6} m {:.6} {y:.6} l",
            crop.min.x, crop.max.x
        )?;
    }
    content.push_str("S\nQ\n");
    object.dict_put(
        "Contents",
        document.add_stream(&Buffer::from_bytes(content.as_bytes())?, None, false)?,
    )?;
    let mut bytes = Vec::new();
    document.write_to(&mut bytes)?;
    Ok(bytes)
}

fn flatten_embedded_files(document: &PdfDocument) -> Result<()> {
    let Some(mut names) = document.catalog()?.get_dict("Names")? else {
        return Ok(());
    };
    if names.get_dict("EmbeddedFiles")?.is_none() {
        return Ok(());
    }
    let flat = document.load_name_tree(PdfObject::new_name("EmbeddedFiles")?)?;
    let mut array = document.new_array()?;
    for entry in flat.dict_iter()? {
        let (key, value) = entry?;
        array.array_push(PdfObject::new_string(&String::from_utf8(key.as_name()?)?)?)?;
        array.array_push(value)?;
    }
    let mut tree = document.new_dict()?;
    tree.dict_put("Names", array)?;
    names.dict_put("EmbeddedFiles", tree)?;
    Ok(())
}

fn sort_embedded_files(document: &PdfDocument) -> Result<()> {
    let mut tree = required(&required(&document.catalog()?, "Names")?, "EmbeddedFiles")?;
    let array = required(&tree, "Names")?;
    let mut entries = Vec::new();
    for i in (0..array.len()?).step_by(2) {
        entries.push((
            array
                .get_array(i as i32)?
                .context("Missing attachment name")?
                .as_string()?,
            array.get_array(i as i32 + 1)?.context("Missing filespec")?,
        ));
    }
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut sorted = document.new_array()?;
    for (name, value) in entries {
        sorted.array_push(PdfObject::new_string(&name)?)?;
        sorted.array_push(value)?;
    }
    tree.dict_put("Names", sorted)?;
    Ok(())
}

fn attachment_names(document: &PdfDocument) -> Result<Vec<String>> {
    let flat = document.load_name_tree(PdfObject::new_name("EmbeddedFiles")?)?;
    let mut names = Vec::new();
    if !flat.is_dict()? {
        return Ok(names);
    }
    for entry in flat.dict_iter()? {
        let (key, _) = entry?;
        names.push(String::from_utf8(key.as_name()?)?);
    }
    Ok(names)
}

pub(super) fn read_edit_data(document: &PdfDocument) -> Result<Option<EditData>> {
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
        ensure!(
            length.as_int()? >= 0 && length.as_int()? as usize <= MAX_EDIT_BYTES,
            "Edit attachment exceeds the 8 MiB limit"
        );
    }
    let data = EditData::decode(&stream.read_stream()?)?;
    ensure!(
        &data.filename() == name,
        "Attachment document UUID does not match its name"
    );
    let mut ids = HashSet::new();
    for saved in &data.strokes {
        ensure!(
            ids.insert(saved.stroke.id),
            "Duplicate stroke UUID in attachment"
        );
        let page = document.load_pdf_page(i32::try_from(saved.stroke.page_index)?)?;
        let geometry = page_geometry(&page)?;
        validate_stroke(&saved.stroke, geometry)?;
        let mut matched = Vec::new();
        for annotation in page.annotations() {
            let object = annotation.object();
            if object
                .get_dict("NM")?
                .map(|id| id.as_string())
                .transpose()?
                .as_deref()
                == Some(&saved.stroke.id.to_string())
            {
                matched.push(object);
            }
        }
        ensure!(
            matched.len() == 1,
            "Missing or duplicated annotation {}",
            saved.stroke.id
        );
        let object = &matched[0];
        ensure!(
            is_name(object.get_dict("Subtype")?, b"Ink")?,
            "Annotation is no longer Ink"
        );
        ensure!(
            fingerprint(object)? == saved.annotation_sha256,
            "Annotation fingerprint mismatch for {}",
            saved.stroke.id
        );
        verify_geometry(object, &saved.stroke, geometry)?;
    }
    Ok(Some(data))
}

pub(super) fn edit_data_status(document: &PdfDocument) -> String {
    match read_edit_data(document) {
        Ok(Some(data)) => format!(
            "Edit data v{}: {} verified stroke(s)",
            data.format_version,
            data.strokes.len()
        ),
        Ok(None) => "No Naive Note edit data".to_owned(),
        Err(error) => format!("Edit data ignored: {error:#}"),
    }
}

fn validate_stroke(stroke: &StrokeRecord, geometry: PageGeometry) -> Result<()> {
    ensure!(
        !stroke.id.is_nil()
            && !stroke.modeled_samples.is_empty()
            && !stroke.raw_samples.is_empty()
            && !stroke.outline.is_empty(),
        "Incomplete stroke record"
    );
    let style = &stroke.style;
    ensure!(
        style.max_width_pt.is_finite()
            && style.max_width_pt > 0.0
            && style.max_width_pt < 1000.0
            && (0.0..=1.0).contains(&style.min_width_ratio)
            && style.pressure_gamma.is_finite()
            && style.pressure_gamma > 0.0,
        "Invalid pen style"
    );
    ensure!(
        stroke.processing.coordinate_unit == "paper-pt-top-left",
        "Unsupported stroke coordinates"
    );
    for sample in &stroke.modeled_samples {
        // A stroke keeps its complete trajectory when it leaves its page and
        // re-enters. Skia clips to the paper and the PDF retains its CropBox;
        // page containment is not a validity requirement for an InkList point.
        let pdf = geometry.paper_to_pdf(sample.position);
        ensure!(
            sample.position.is_finite()
                && pdf.is_finite()
                && pdf.x.abs() < 1e9
                && pdf.y.abs() < 1e9
                && (0.0..=1.0).contains(&sample.pressure),
            "Invalid modeled sample"
        );
    }
    for sample in &stroke.raw_samples {
        ensure!(
            sample.position.is_finite()
                && sample.pressure.is_none_or(|p| (0.0..=1.0).contains(&p))
                && sample
                    .tilt_degrees
                    .is_none_or(|a| a.iter().all(|v| v.is_finite()))
                && sample.rotation_degrees.is_none_or(f64::is_finite),
            "Invalid raw sample"
        );
    }
    Ok(())
}

fn appearance(stroke: &StrokeRecord, geometry: PageGeometry) -> Result<([f64; 4], String)> {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut path = String::from("q\n");
    let [r, g, b] = stroke.style.color_rgb.map(|c| f64::from(c) / 255.0);
    writeln!(path, "{r:.8} {g:.8} {b:.8} rg")?;
    let mut point = |p: Point| -> Result<Point> {
        ensure!(p.is_finite(), "Non-finite outline point");
        let pdf = geometry.paper_to_pdf(p);
        ensure!(
            pdf.is_finite() && pdf.x.abs() < 1e9 && pdf.y.abs() < 1e9,
            "Outline exceeds PDF limits"
        );
        bounds[0] = bounds[0].min(pdf.x);
        bounds[1] = bounds[1].min(pdf.y);
        bounds[2] = bounds[2].max(pdf.x);
        bounds[3] = bounds[3].max(pdf.y);
        Ok(pdf)
    };
    for command in &stroke.outline {
        match *command {
            PathCommand::MoveTo(p) => {
                let p = point(p)?;
                writeln!(path, "{:.6} {:.6} m", p.x, p.y)?;
            }
            PathCommand::LineTo(p) => {
                let p = point(p)?;
                writeln!(path, "{:.6} {:.6} l", p.x, p.y)?;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let a = point(control1)?;
                let b = point(control2)?;
                let c = point(to)?;
                writeln!(
                    path,
                    "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c",
                    a.x, a.y, b.x, b.y, c.x, c.y
                )?;
            }
            PathCommand::Close => path.push_str("h\n"),
        }
    }
    // Rect/BBox share absolute PDF user-space coordinates and identity Matrix;
    // the PDF annotation appearance mapping is therefore identity as well.
    let margin = 0.5 / geometry.user_unit();
    bounds[0] -= margin;
    bounds[1] -= margin;
    bounds[2] += margin;
    bounds[3] += margin;
    ensure!(
        bounds.iter().all(|v| v.is_finite()) && bounds[2] > bounds[0] && bounds[3] > bounds[1],
        "Invalid appearance bounds"
    );
    path.push_str("f\nQ\n");
    Ok((bounds, path))
}

fn verify_geometry(
    object: &PdfObject,
    stroke: &StrokeRecord,
    geometry: PageGeometry,
) -> Result<()> {
    let list = required(object, "InkList")?;
    ensure!(list.len()? == 1, "Unexpected InkList stroke count");
    let points = list.get_array(0)?.context("Missing InkList points")?;
    ensure!(
        points.len()? == stroke.modeled_samples.len() * 2,
        "Unexpected InkList point count"
    );
    for (i, sample) in stroke.modeled_samples.iter().enumerate() {
        let expected = geometry.paper_to_pdf(sample.position);
        for (j, value) in [expected.x, expected.y].into_iter().enumerate() {
            let actual = points
                .get_array((i * 2 + j) as i32)?
                .context("Missing ink coordinate")?
                .as_float()?;
            ensure!(
                (f64::from(actual) - value).abs() <= 0.002_f64.max(value.abs() * 1e-6),
                "InkList coordinate changed"
            );
        }
    }
    let (rect, contents) = appearance(stroke, geometry)?;
    let ap = normal_appearance(object)?;
    ensure!(
        ap.read_stream()? == contents.as_bytes(),
        "Pressure appearance was replaced"
    );
    ensure!(
        floats(&required(object, "Rect")?)? == rect.map(|v| v as f32),
        "Annotation rectangle changed"
    );
    ensure!(
        floats(&required(&ap, "BBox")?)? == rect.map(|v| v as f32),
        "Appearance BBox changed"
    );
    ensure!(
        floats(&required(&ap, "Matrix")?)? == [1., 0., 0., 1., 0., 0.],
        "Appearance matrix changed"
    );
    ensure!(
        required(object, "F")?.as_int()? & 4 != 0,
        "Annotation is not printable"
    );
    Ok(())
}

fn fingerprint(object: &PdfObject) -> Result<String> {
    let ap = normal_appearance(object)?;
    let ink = required(object, "InkList")?;
    let mut lines = Vec::new();
    for line in ink.array_iter()? {
        lines.push(floats(&line?)?);
    }
    let mut bytes = Vec::new();
    ciborium::into_writer(
        &(
            required(object, "NM")?.as_string()?,
            required(object, "Subtype")?.as_name()?,
            required(object, "F")?.as_int()?,
            floats(&required(object, "C")?)?,
            floats(&required(object, "Rect")?)?,
            lines,
            required(&ap, "Subtype")?.as_name()?,
            floats(&required(&ap, "BBox")?)?,
            floats(&required(&ap, "Matrix")?)?,
            ap.read_stream()?,
        ),
        &mut bytes,
    )?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn normal_appearance(object: &PdfObject) -> Result<PdfObject> {
    let ap = required(&required(object, "AP")?, "N")?;
    ensure!(
        ap.is_stream()? && is_name(ap.get_dict("Subtype")?, b"Form")?,
        "Missing normal Form appearance"
    );
    Ok(ap)
}

fn required(object: &PdfObject, key: &str) -> Result<PdfObject> {
    object
        .get_dict(key)?
        .with_context(|| format!("Missing PDF entry /{key}"))
}

fn is_name(object: Option<PdfObject>, name: &[u8]) -> Result<bool> {
    Ok(object.map(|o| o.as_name()).transpose()?.as_deref() == Some(name))
}

fn floats(object: &PdfObject) -> Result<Vec<f32>> {
    object.array_iter()?.map(|o| Ok(o?.as_float()?)).collect()
}

fn numbers(document: &PdfDocument, values: &[f64]) -> Result<PdfObject> {
    let mut array = document.new_array()?;
    for value in values {
        array.array_push(PdfObject::new_real(*value as f32)?)?;
    }
    Ok(array)
}

#[cfg(test)]
mod tests;
