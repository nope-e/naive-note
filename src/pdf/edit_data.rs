use std::io::{Cursor, Read};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::StrokeRecord;

pub(super) const FORMAT_VERSION: u32 = 1;
pub(super) const MAX_EDIT_BYTES: usize = 8 * 1024 * 1024;

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
}

impl EditData {
    pub fn filename(&self) -> String {
        format!("naive-note.{}.cbor.zst", self.document_id)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut cbor = Vec::new();
        ciborium::into_writer(self, &mut cbor).context("Encode edit data as CBOR")?;
        ensure!(
            cbor.len() <= MAX_EDIT_BYTES,
            "Edit data exceeds the 8 MiB probe limit"
        );
        zstd::stream::encode_all(cbor.as_slice(), 3).context("Compress edit data with Zstd")
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_EDIT_BYTES,
            "Compressed edit data exceeds the 8 MiB probe limit"
        );
        let mut decoder =
            zstd::stream::read::Decoder::new(bytes).context("Invalid Zstd edit data")?;
        decoder.window_log_max(23)?;
        let mut cbor = Vec::new();
        decoder
            .take(MAX_EDIT_BYTES as u64 + 1)
            .read_to_end(&mut cbor)?;
        ensure!(
            cbor.len() <= MAX_EDIT_BYTES,
            "Decoded edit data exceeds the 8 MiB probe limit"
        );
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
        Ok(data)
    }
}
