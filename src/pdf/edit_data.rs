use std::io::{BufWriter, Cursor};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::{ReferenceLayer, StrokeRecord};

pub(super) const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct SavedStroke {
    pub stroke: StrokeRecord,
    pub annotation_sha256: String,
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
        // uncompressed copy of all capture samples and geometry during encoding.
        let encoder = zstd::stream::write::Encoder::new(Vec::new(), 3)
            .context("Create Zstd edit data encoder")?;
        let mut writer = BufWriter::new(encoder);
        ciborium::into_writer(self, &mut writer).context("Encode edit data as CBOR")?;
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
            data.format_version == FORMAT_VERSION,
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
