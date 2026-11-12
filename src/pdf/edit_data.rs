use std::io::{BufWriter, Cursor};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::{ReferenceLayer, StrokeRecord};

mod compact;

pub(super) const FORMAT_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct SavedStroke {
    pub stroke: StrokeRecord,
    pub annotation_sha256: String,
}

impl SavedStroke {
    pub fn new(stroke: &StrokeRecord, annotation_sha256: String) -> Self {
        Self {
            // Capture samples are only needed while modeling a live stroke.
            // Keep exact confirmed geometry, style and transform for later edits,
            // without cloning or persisting the redundant device history.
            stroke: StrokeRecord {
                id: stroke.id,
                page_index: stroke.page_index,
                edit_data_verified: stroke.edit_data_verified,
                pdf_annotation: None,
                raw_samples: Vec::new(),
                edit_transform: stroke.edit_transform,
                modeled_samples: stroke.modeled_samples.clone(),
                outline: stroke.outline.clone(),
                style: stroke.style.clone(),
                processing: stroke.processing.clone(),
            },
            annotation_sha256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct EditData {
    pub format_version: u32,
    pub document_id: Uuid,
    pub strokes: Vec<SavedStroke>,
    #[serde(default)]
    pub reference_lines: Option<ReferenceLayer>,
}

impl EditData {
    pub fn filename(&self) -> String {
        format!("naive-note.{}.cbor.zst", self.document_id)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        // Stream CBOR into Zstd so large documents do not need an additional
        // uncompressed copy of the confirmed geometry during encoding.
        let encoder = zstd::stream::write::Encoder::new(Vec::new(), 3)
            .context("Create Zstd edit data encoder")?;
        let mut writer = BufWriter::new(encoder);
        if self.format_version >= 3 {
            ciborium::into_writer(&compact::EditDataRef::new(self), &mut writer)
                .context("Encode compact edit data as CBOR")?;
        } else {
            ciborium::into_writer(self, &mut writer).context("Encode edit data as CBOR")?;
        }
        let encoder = writer
            .into_inner()
            .map_err(|error| error.into_error())
            .context("Flush CBOR edit data")?;
        encoder.finish().context("Compress edit data with Zstd")
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let cbor = zstd::stream::decode_all(bytes).context("Invalid Zstd edit data")?;
        let mut cursor = Cursor::new(&cbor);
        let data: Self = ciborium::from_reader(&mut cursor).context("Invalid CBOR edit data")?;
        ensure!(
            cursor.position() == cbor.len() as u64,
            "Trailing CBOR edit data"
        );
        ensure!(
            (1..=FORMAT_VERSION).contains(&data.format_version),
            "Unsupported edit data version {}",
            data.format_version
        );
        ensure!(!data.document_id.is_nil(), "Invalid document UUID");
        if let Some(layer) = data.reference_lines {
            layer.lines.validate()?;
        }
        Ok(data)
    }
}
