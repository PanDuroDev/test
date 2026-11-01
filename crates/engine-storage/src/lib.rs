//! Versioned, bounded RASM document encoding.
//!
//! The codec is filesystem-agnostic. Callers own file I/O and atomic
//! replacement; this crate validates the complete byte container first.

use rasm_engine_document::{Document, DocumentError, DocumentSnapshot};
use serde::{Deserialize, Serialize};
use std::fmt;

const MAGIC: &[u8; 4] = b"RASM";
const FILE_VERSION: u16 = 1;
const SCHEMA_VERSION: u16 = 1;
const HEADER_LEN: usize = 20;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
const MAX_LAYERS: usize = 4_096;
const MAX_STROKES: usize = 1_000_000;
const MAX_TOTAL_SAMPLES: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    TruncatedHeader,
    InvalidMagic,
    UnsupportedFileVersion(u16),
    UnsupportedFlags(u16),
    PayloadTooLarge,
    LengthMismatch,
    ChecksumMismatch,
    InvalidJson,
    UnsupportedSchemaVersion(u16),
    ResourceLimitExceeded,
    InvalidDocument(DocumentError),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader => f.write_str("file is shorter than the RASM header"),
            Self::InvalidMagic => f.write_str("file does not have RASM magic bytes"),
            Self::UnsupportedFileVersion(v) => write!(f, "unsupported file version: {v}"),
            Self::UnsupportedFlags(v) => write!(f, "unsupported file flags: {v:#06x}"),
            Self::PayloadTooLarge => f.write_str("payload exceeds the configured size limit"),
            Self::LengthMismatch => f.write_str("payload length does not match file length"),
            Self::ChecksumMismatch => f.write_str("payload checksum does not match"),
            Self::InvalidJson => f.write_str("payload is not a valid schema document"),
            Self::UnsupportedSchemaVersion(v) => write!(f, "unsupported schema version: {v}"),
            Self::ResourceLimitExceeded => f.write_str("document exceeds configured resource limits"),
            Self::InvalidDocument(e) => write!(f, "document validation failed: {e}"),
        }
    }
}
impl std::error::Error for StorageError {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema_version: u16,
    document: DocumentSnapshot,
}

/// Encode a document into the versioned RASM v1 container.
pub fn encode(document: &Document) -> Result<Vec<u8>, StorageError> {
    let snapshot = document.snapshot();
    validate_resource_limits(&snapshot)?;
    let payload = serde_json::to_vec(&Payload {
        schema_version: SCHEMA_VERSION,
        document: snapshot,
    }).map_err(|_| StorageError::InvalidJson)?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(StorageError::PayloadTooLarge);
    }

    let payload_len = u64::try_from(payload.len()).map_err(|_| StorageError::PayloadTooLarge)?;
    let checksum = crc32fast::hash(&payload);
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

/// Validate and decode a complete RASM container into a fresh document.
pub fn decode(bytes: &[u8]) -> Result<Document, StorageError> {
    if bytes.len() < HEADER_LEN {
        return Err(StorageError::TruncatedHeader);
    }
    if &bytes[..4] != MAGIC {
        return Err(StorageError::InvalidMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != FILE_VERSION {
        return Err(StorageError::UnsupportedFileVersion(version));
    }
    let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
    if flags != 0 {
        return Err(StorageError::UnsupportedFlags(flags));
    }

    let declared = u64::from_le_bytes(bytes[8..16].try_into().expect("fixed header"));
    let payload_len = usize::try_from(declared).map_err(|_| StorageError::PayloadTooLarge)?;
    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(StorageError::PayloadTooLarge);
    }
    let expected_len = HEADER_LEN.checked_add(payload_len).ok_or(StorageError::PayloadTooLarge)?;
    if bytes.len() != expected_len {
        return Err(StorageError::LengthMismatch);
    }

    let checksum = u32::from_le_bytes(bytes[16..20].try_into().expect("fixed header"));
    let payload_bytes = &bytes[HEADER_LEN..];
    if crc32fast::hash(payload_bytes) != checksum {
        return Err(StorageError::ChecksumMismatch);
    }

    let payload: Payload = serde_json::from_slice(payload_bytes).map_err(|_| StorageError::InvalidJson)?;
    if payload.schema_version != SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion(payload.schema_version));
    }
    validate_resource_limits(&payload.document)?;
    Document::from_snapshot(payload.document).map_err(StorageError::InvalidDocument)
}

fn validate_resource_limits(snapshot: &DocumentSnapshot) -> Result<(), StorageError> {
    if snapshot.layers.len() > MAX_LAYERS {
        return Err(StorageError::ResourceLimitExceeded);
    }
    let mut strokes = 0_usize;
    let mut samples = 0_usize;
    for layer in &snapshot.layers {
        strokes = strokes.checked_add(layer.strokes.len()).ok_or(StorageError::ResourceLimitExceeded)?;
        for stroke in &layer.strokes {
            samples = samples.checked_add(stroke.samples.len()).ok_or(StorageError::ResourceLimitExceeded)?;
        }
    }
    if strokes > MAX_STROKES || samples > MAX_TOTAL_SAMPLES {
        return Err(StorageError::ResourceLimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, StorageError, HEADER_LEN};
    use glam::DVec2;
    use rasm_engine_document::{Document, Edit, EditResult, History};
    use rasm_engine_ink::{BrushConfig, StrokeSample};

    fn document() -> Document {
        let mut doc = Document::new(2048, 1536).unwrap();
        let mut history = History::default();
        let layer = match history.execute(&mut doc, Edit::add_layer("Ink")).unwrap() {
            EditResult::LayerAdded(id) => id,
            _ => unreachable!(),
        };
        let samples = vec![
            StrokeSample::new(DVec2::new(-12.5, 7.25), Some(0.2), 10).unwrap(),
            StrokeSample::new(DVec2::new(44.0, 99.5), None, 20).unwrap(),
        ];
        history.execute(&mut doc, Edit::add_stroke(
            layer, samples, BrushConfig::new(13.0, 0.1, 0.8, 0.3, 10_000).unwrap()
        )).unwrap();
        history.execute(&mut doc, Edit::set_layer_opacity(layer, 0.65)).unwrap();
        doc
    }

    fn replace_payload(bytes: &mut Vec<u8>, payload: &[u8]) {
        bytes.truncate(HEADER_LEN);
        bytes[8..16].copy_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes[16..20].copy_from_slice(&crc32fast::hash(payload).to_le_bytes());
        bytes.extend_from_slice(payload);
    }

    #[test]
    fn round_trip_preserves_document_and_next_ids() {
        let original = document();
        let restored = decode(&encode(&original).unwrap()).unwrap();
        assert_eq!(restored, original);
        let mut restored = restored;
        let mut history = History::default();
        let id = match history.execute(&mut restored, Edit::add_layer("After reopen")).unwrap() {
            EditResult::LayerAdded(id) => id,
            _ => unreachable!(),
        };
        assert!(id.get() > original.snapshot().layers.iter().map(|l| l.id).max().unwrap());
    }

    #[test]
    fn rejects_header_magic_version_flags_and_trailing_bytes() {
        assert_eq!(decode(b"RASM"), Err(StorageError::TruncatedHeader));
        let valid = encode(&document()).unwrap();
        let mut bytes = valid.clone();
        bytes[0] = b'X';
        assert_eq!(decode(&bytes), Err(StorageError::InvalidMagic));
        let mut bytes = valid.clone();
        bytes[4..6].copy_from_slice(&99_u16.to_le_bytes());
        assert_eq!(decode(&bytes), Err(StorageError::UnsupportedFileVersion(99)));
        let mut bytes = valid.clone();
        bytes[6..8].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(decode(&bytes), Err(StorageError::UnsupportedFlags(1)));
        let mut bytes = valid;
        bytes.push(0);
        assert_eq!(decode(&bytes), Err(StorageError::LengthMismatch));
    }

    #[test]
    fn rejects_truncation_and_checksum_corruption() {
        let mut bytes = encode(&document()).unwrap();
        bytes.pop();
        assert_eq!(decode(&bytes), Err(StorageError::LengthMismatch));
        let mut bytes = encode(&document()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        assert_eq!(decode(&bytes), Err(StorageError::ChecksumMismatch));
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let mut bytes = encode(&document()).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&bytes[HEADER_LEN..]).unwrap();
        json["schema_version"] = serde_json::json!(77);
        replace_payload(&mut bytes, &serde_json::to_vec(&json).unwrap());
        assert_eq!(decode(&bytes), Err(StorageError::UnsupportedSchemaVersion(77)));
    }

    #[test]
    fn rejects_invalid_document_snapshot() {
        let mut bytes = encode(&document()).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&bytes[HEADER_LEN..]).unwrap();
        json["document"]["layers"][0]["opacity"] = serde_json::json!(2.0);
        replace_payload(&mut bytes, &serde_json::to_vec(&json).unwrap());
        assert!(matches!(decode(&bytes), Err(StorageError::InvalidDocument(_))));
    }

    #[test]
    fn rejects_unknown_json_fields() {
        let mut bytes = encode(&document()).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&bytes[HEADER_LEN..]).unwrap();
        json["unexpected"] = serde_json::json!(true);
        replace_payload(&mut bytes, &serde_json::to_vec(&json).unwrap());
        assert_eq!(decode(&bytes), Err(StorageError::InvalidJson));
    }

    #[test]
    fn rejects_payload_exceeding_declared_limit() {
        let mut bytes = vec![0; HEADER_LEN];
        bytes[..4].copy_from_slice(b"RASM");
        bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&((64_u64 * 1024 * 1024) + 1).to_le_bytes());
        assert_eq!(decode(&bytes), Err(StorageError::PayloadTooLarge));
    }
}
