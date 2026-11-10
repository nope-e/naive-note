//! Object-level edits preserve standard dictionaries and wrap their current AP.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use mupdf::{
    Buffer,
    pdf::{PdfDocument, PdfObject},
};

use crate::document::{PageGeometry, PdfAnnotationSource, Point, StrokeRecord};

use super::{array, page_geometry};

type Affine = [f64; 6];

/// The native and standard object layers share one per-page stacking order.
pub(in crate::pdf) fn order_objects(
    document: &PdfDocument,
    sources: &[Arc<PdfAnnotationSource>],
    strokes: &[Arc<StrokeRecord>],
) -> Result<()> {
    let mut managed: HashSet<_> = sources.iter().map(|source| source.xref).collect();
    let mut popups = HashMap::new();
    for source in sources {
        if let Some(object) = document.xref_object(source.xref)?
            && let Some(popup) = object.get_dict("Popup")?
        {
            managed.insert(popup.as_indirect()?);
            popups.insert(source.xref, popup);
        }
    }
    let native_ids: HashSet<_> = strokes
        .iter()
        .filter(|s| s.pdf_annotation.is_none())
        .map(|s| s.id.to_string())
        .collect();
    for index in 0..document.page_count()? {
        let mut page = document.find_page(index)?;
        let mut ordered = document.new_array()?;
        let mut standard = HashMap::new();
        let mut native = HashMap::new();
        if let Some(annotations) = page.get_dict("Annots")? {
            for entry in annotations.array_iter()? {
                let entry = entry?;
                let xref = entry.as_indirect()?;
                let name = entry.get_dict("NM")?.and_then(|name| name.as_string().ok());
                if managed.contains(&xref) {
                    standard.insert(xref, entry);
                } else if let Some(name) = name.filter(|name| native_ids.contains(name)) {
                    native.insert(name, entry);
                } else {
                    ordered.array_push(entry)?;
                }
            }
        }
        for stroke in strokes
            .iter()
            .filter(|stroke| stroke.page_index == index as u32)
        {
            let object = if let Some(source) = &stroke.pdf_annotation {
                standard
                    .remove(&source.xref)
                    .context("Missing standard object while ordering")?
            } else {
                native
                    .remove(&stroke.id.to_string())
                    .context("Missing native object while ordering")?
            };
            ordered.array_push(object)?;
            if let Some(source) = &stroke.pdf_annotation
                && let Some(popup) = popups.get(&source.xref)
            {
                ordered.array_push(popup.clone())?;
            }
        }
        page.dict_put("Annots", ordered)?;
    }
    Ok(())
}

/// Serialization may not silently change the standard objects we just edited.
pub(in crate::pdf) fn verify_saved(
    expected: &PdfDocument,
    actual: &PdfDocument,
    sources: &[Arc<PdfAnnotationSource>],
) -> Result<()> {
    let source_ids: HashSet<_> = sources.iter().map(|source| source.xref).collect();
    let expected_pages = memberships(expected, &source_ids)?;
    let actual_pages = memberships(actual, &source_ids)?;
    for source in sources {
        let pages = expected_pages.get(&source.xref);
        ensure!(
            actual_pages.get(&source.xref) == pages,
            "Saved annotation page membership changed"
        );
        if pages.is_none() {
            continue;
        }
        let before = expected
            .xref_object(source.xref)?
            .context("Missing expected annotation")?;
        let after = actual
            .xref_object(source.xref)?
            .context("Missing saved annotation")?;
        for key in [
            "Subtype",
            "Rect",
            "InkList",
            "QuadPoints",
            "Vertices",
            "L",
            "CL",
            "NM",
            "F",
            "CA",
            "C",
            "AP",
        ] {
            ensure!(
                before.get_dict(key)?.map(|o| o.to_string())
                    == after.get_dict(key)?.map(|o| o.to_string()),
                "Saved annotation /{key} changed"
            );
        }
        if let Some(ap) = before.get_dict("AP")?
            && let Some(normal) = ap.get_dict("N")?
            && normal.is_stream()?
        {
            let saved_ap = after
                .get_dict("AP")?
                .context("Missing saved AP")?
                .get_dict("N")?
                .context("Missing saved normal AP")?;
            ensure!(
                normal.read_stream()? == saved_ap.read_stream()?,
                "Saved standard annotation appearance changed"
            );
        }
    }
    Ok(())
}

fn memberships(document: &PdfDocument, ids: &HashSet<i32>) -> Result<HashMap<i32, Vec<i32>>> {
    let mut pages = HashMap::<_, Vec<_>>::new();
    if ids.is_empty() {
        return Ok(pages);
    }
    for index in 0..document.page_count()? {
        if let Some(annotations) = document.find_page(index)?.get_dict("Annots")? {
            for entry in annotations.array_iter()? {
                let xref = entry?.as_indirect()?;
                if ids.contains(&xref) {
                    pages.entry(xref).or_default().push(index);
                }
            }
        }
    }
    Ok(pages)
}

pub(in crate::pdf) fn apply_edits(
    document: &mut PdfDocument,
    sources: &[Arc<PdfAnnotationSource>],
    strokes: &[Arc<StrokeRecord>],
) -> Result<()> {
    let baseline: HashSet<_> = sources.iter().map(|source| source.xref).collect();
    let original_pages = memberships(document, &baseline)?;
    let mut live = HashMap::new();
    for stroke in strokes {
        if let Some(source) = &stroke.pdf_annotation {
            ensure!(baseline.contains(&source.xref), "Unknown source annotation");
            ensure!(
                live.insert(source.xref, stroke).is_none(),
                "Duplicate source annotation"
            );
            ensure!(
                stroke.page_index < document.page_count()? as u32,
                "Invalid annotation destination"
            );
        }
    }
    let mut destinations = HashMap::new();
    for source in sources {
        let mut object = document
            .xref_object(source.xref)?
            .context("Missing source annotation")?;
        let source_page = document.load_pdf_page(source.page_index as i32)?;
        ensure!(
            original_pages.get(&source.xref).map(Vec::as_slice)
                == Some(&[source.page_index as i32]),
            "Annotation no longer belongs to its source page"
        );
        let destination = live.get(&source.xref).map(|stroke| stroke.page_index);
        destinations.insert(source.xref, destination);
        if let Some(popup) = object.get_dict("Popup")? {
            destinations.insert(popup.as_indirect()?, destination);
        }
        let Some(stroke) = live.get(&source.xref) else {
            continue;
        };
        if stroke.edit_transform.is_identity() && stroke.page_index == source.page_index {
            continue;
        }
        let destination_page = document.load_pdf_page(stroke.page_index as i32)?;
        let transform = pdf_transform(
            page_geometry(&source_page)?,
            page_geometry(&destination_page)?,
            stroke,
        );
        transform_annotation(document, &mut object, transform, source)?;
        object.dict_put("P", document.find_page(stroke.page_index as i32)?)?;
        if let Some(mut popup) = object.get_dict("Popup")? {
            if let Some(rect) = popup.get_dict("Rect")? {
                popup.dict_put("Rect", array(document, &mapped_rect(&rect, transform)?)?)?;
            }
            popup.dict_put("P", document.find_page(stroke.page_index as i32)?)?;
        }
    }
    // Rebuild page membership from the immutable source. Popups follow their
    // parent; unselected annotations, links, widgets and relationships survive.
    let mut moved = Vec::new();
    for index in 0..document.page_count()? {
        let mut page = document.find_page(index)?;
        let Some(annotations) = page.get_dict("Annots")? else {
            continue;
        };
        let mut kept = document.new_array()?;
        for entry in annotations.array_iter()? {
            let entry = entry?;
            match destinations.get(&entry.as_indirect()?) {
                Some(Some(destination)) if *destination != index as u32 => {
                    moved.push((*destination, entry))
                }
                Some(None) => {}
                _ => kept.array_push(entry)?,
            }
        }
        page.dict_put("Annots", kept)?;
    }
    for (destination, object) in moved {
        let mut page = document.find_page(destination as i32)?;
        let mut annotations = page.get_dict("Annots")?.unwrap_or(document.new_array()?);
        annotations.array_push(object)?;
        page.dict_put("Annots", annotations)?;
    }
    Ok(())
}

fn pdf_transform(source: PageGeometry, destination: PageGeometry, stroke: &StrokeRecord) -> Affine {
    let map =
        |point| destination.paper_to_pdf(stroke.edit_transform.apply(source.pdf_to_paper(point)));
    let origin = map(Point::default());
    let x = map(Point::new(1., 0.));
    let y = map(Point::new(0., 1.));
    [
        x.x - origin.x,
        x.y - origin.y,
        y.x - origin.x,
        y.y - origin.y,
        origin.x,
        origin.y,
    ]
}

fn transform_annotation(
    document: &mut PdfDocument,
    object: &mut PdfObject,
    transform: Affine,
    source: &PdfAnnotationSource,
) -> Result<()> {
    let old_rect = object
        .get_dict("Rect")?
        .context("Missing annotation rectangle")?;
    let new_rect = mapped_rect(&old_rect, transform)?;
    let old_rect = values(&old_rect)?;
    let ap = if let Some(ap) = object.get_dict("AP")? {
        ap
    } else {
        let generated = PdfDocument::from_bytes(&source.appearance_pdf)?;
        let generated = generated
            .load_pdf_page(0)?
            .annotations()
            .next()
            .context("Missing generated annotation")?
            .object();
        document.graft_object(
            &generated
                .get_dict("AP")?
                .context("Missing generated appearance")?,
        )?
    };
    let mut transformed_ap = document.new_dict()?;
    for entry in ap.dict_iter()? {
        let (key, appearance) = entry?;
        let key = String::from_utf8(key.as_name()?)?;
        transformed_ap.dict_put(
            key.as_str(),
            transform_appearance(document, &appearance, &old_rect, new_rect, transform)?,
        )?;
    }
    object.dict_put("AP", transformed_ap)?;
    // Transform standard geometry as well as AP so another reader sees the
    // current coordinates even if it later regenerates an appearance.
    if let Some(ink_list) = object.get_dict("InkList")? {
        let mut transformed = document.new_array()?;
        for line in ink_list.array_iter()? {
            transformed.array_push(map_points(document, &line?, transform)?)?;
        }
        object.dict_put("InkList", transformed)?;
    }
    for key in ["QuadPoints", "Vertices", "L", "CL"] {
        if let Some(points) = object.get_dict(key)? {
            object.dict_put(key, map_points(document, &points, transform)?)?;
        }
    }
    object.dict_put("Rect", array(document, &new_rect)?)?;
    Ok(())
}

fn transform_appearance(
    document: &mut PdfDocument,
    appearance: &PdfObject,
    old_rect: &[f64],
    new_rect: [f64; 4],
    transform: Affine,
) -> Result<PdfObject> {
    if !appearance.is_stream()? {
        let mut states = document.new_dict()?;
        for entry in appearance.dict_iter()? {
            let (key, value) = entry?;
            let key = String::from_utf8(key.as_name()?)?;
            states.dict_put(
                key.as_str(),
                transform_appearance(document, &value, old_rect, new_rect, transform)?,
            )?;
        }
        return Ok(states);
    }
    let matrix = appearance
        .get_dict("Matrix")?
        .map(|matrix| values(&matrix))
        .transpose()?
        .unwrap_or(vec![1., 0., 0., 1., 0., 0.]);
    let matrix: Affine = matrix
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid AP matrix"))?;
    let bounds = mapped_rect(
        &appearance.get_dict("BBox")?.context("Missing AP bounds")?,
        matrix,
    )?;
    ensure!(
        old_rect.len() == 4 && bounds[2] > bounds[0] && bounds[3] > bounds[1],
        "Invalid AP placement"
    );
    let sx = (old_rect[2] - old_rect[0]) / (bounds[2] - bounds[0]);
    let sy = (old_rect[3] - old_rect[1]) / (bounds[3] - bounds[1]);
    let placement = [
        sx,
        0.,
        0.,
        sy,
        old_rect[0] - bounds[0] * sx,
        old_rect[1] - bounds[1] * sy,
    ];
    // The original form applies its own Matrix when invoked by Do.
    let combined = followed_by(placement, transform);
    let mut form = document.new_dict()?;
    form.dict_put("Type", PdfObject::new_name("XObject")?)?;
    form.dict_put("Subtype", PdfObject::new_name("Form")?)?;
    form.dict_put("BBox", array(document, &new_rect)?)?;
    form.dict_put("Matrix", array(document, &[1., 0., 0., 1., 0., 0.])?)?;
    let mut objects = document.new_dict()?;
    objects.dict_put("NNOriginal", appearance.clone())?;
    let mut resources = document.new_dict()?;
    resources.dict_put("XObject", objects)?;
    form.dict_put("Resources", resources)?;
    let [a, b, c, d, e, f] = combined;
    let content =
        format!("q {a:.10} {b:.10} {c:.10} {d:.10} {e:.10} {f:.10} cm /NNOriginal Do Q\n");
    Ok(document.add_stream(&Buffer::from_bytes(content.as_bytes())?, Some(&form), false)?)
}

fn followed_by(first: Affine, next: Affine) -> Affine {
    let [a, b, c, d, e, f] = first;
    let [g, h, i, j, k, l] = next;
    [
        a * g + b * i,
        a * h + b * j,
        c * g + d * i,
        c * h + d * j,
        e * g + f * i + k,
        e * h + f * j + l,
    ]
}

fn apply(transform: Affine, point: Point) -> Point {
    let [a, b, c, d, e, f] = transform;
    Point::new(a * point.x + c * point.y + e, b * point.x + d * point.y + f)
}

fn values(object: &PdfObject) -> Result<Vec<f64>> {
    object
        .array_iter()?
        .map(|value| Ok(value?.as_float()? as f64))
        .collect()
}

fn mapped_rect(rect: &PdfObject, transform: Affine) -> Result<[f64; 4]> {
    let rect = values(rect)?;
    ensure!(rect.len() == 4, "Invalid annotation rectangle");
    let points = [
        Point::new(rect[0], rect[1]),
        Point::new(rect[2], rect[1]),
        Point::new(rect[2], rect[3]),
        Point::new(rect[0], rect[3]),
    ]
    .map(|point| apply(transform, point));
    Ok(points.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |bounds, p| {
            [
                bounds[0].min(p.x),
                bounds[1].min(p.y),
                bounds[2].max(p.x),
                bounds[3].max(p.y),
            ]
        },
    ))
}

fn map_points(document: &PdfDocument, points: &PdfObject, transform: Affine) -> Result<PdfObject> {
    let points = values(points)?;
    ensure!(
        points.len().is_multiple_of(2),
        "Invalid annotation coordinates"
    );
    let mapped = points
        .as_chunks::<2>()
        .0
        .iter()
        .flat_map(|pair| {
            let point = apply(transform, Point::new(pair[0], pair[1]));
            [point.x, point.y]
        })
        .collect::<Vec<_>>();
    array(document, &mapped)
}
