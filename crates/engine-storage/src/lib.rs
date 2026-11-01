//! Versioned, renderer-independent RASM project document codec.
//!
//! Version 1 uses a fixed binary header followed by a checksummed UTF-8 JSON
//! payload. The codec does not perform filesystem I/O; atomic saves and recovery
//! belong to a separate layer.

use std::fmt;

use rasm_engine_document::{
    BrushSnapshot, Document, DocumentError, DocumentSnapshot, LayerSnapshot, StrokeSampleSnapshot,
    StrokeSnapshot,
};
use serde::{Deserialize, Serialize};

/// Four-byte file signature at the beginning of every project file.
pub const FILE_MAGIC: [u8; 4] = *b"RASM";

/// Current binary container version.
pub const FILE_VERSION: u16 = 1;

/// Current JSON document schema version.
pub const SCHEMA_VERSION: u16 = 1;

/// Fixed header size: magic, version, flags, payload length, and CRC32.
pub const HEADER_SIZE: usize = 20;

/// Maximum payload accepted by the decoder or emitted by the encoder.
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;

const SUPPORTED_FLAGS: u16 = 0;

/// Errors returned while encoding or decoding a project document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    TruncatedHeader,
    InvalidMagic,
    UnsupportedFileVersion(u16),
    UnsupportedFlags(u16),
    PayloadTooLarge,
    PayloadLengthOverflow,
    TruncatedPayload,
    TrailingBytes,
    ChecksumMismatch,
    InvalidJson,
    SerializationFailed,
    UnsupportedSchemaVersion(u16),
    InvalidDocument(DocumentError),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TruncatedHeader => "project file is shorter than the fixed header",
            Self::InvalidMagic => "project file magic does not match RASM",
            Self::UnsupportedFileVersion(_) => "project container version is not supported",
            Self::UnsupportedFlags(_) => "project container uses unsupported flags",
            Self::PayloadTooLarge => "project payload exceeds the configured size limit",
            Self::PayloadLengthOverflow => "project payload length cannot be represented",
            Self::TruncatedPayload => "project payload is shorter than its declared length",
            Self::TrailingBytes => "project file contains bytes after the declared payload",
            Self::ChecksumMismatch => "project payload checksum does not match",
            Self::InvalidJson => "project payload is not valid JSON for the stored schema",
            Self::SerializationFailed => "document could not be serialized",
            Self::UnsupportedSchemaVersion(_) => "document schema version is not supported",
            Self::InvalidDocument(_) => "project payload violates document invariants",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for StorageError {}

/// Encodes a document as a version 1 project byte buffer.
pub fn encode(document: &Document) -> Result<Vec<u8>, StorageError> {
    let stored = StoredDocument::from_snapshot(document.snapshot())?;
    let payload = serde_json::to_vec(&stored).map_err(|_| StorageError::SerializationFailed)?;
    wrap_payload(&payload)
}

/// Decodes and validates a versioned project byte buffer.
pub fn decode(bytes: &[u8]) -> Result<Document, StorageError> {
    if bytes.len() < HEADER_SIZE {
        return Err(StorageError::TruncatedHeader);
    }
    if bytes[0..4] != FILE_MAGIC {
        return Err(StorageError::InvalidMagic);
    }

    let file_version = read_u16(bytes, 4);
    if file_version != FILE_VERSION {
        return Err(StorageError::UnsupportedFileVersion(file_version));
    }

    let flags = read_u16(bytes, 6);
    if flags & !SUPPORTED_FLAGS != 0 {
        return Err(StorageError::UnsupportedFlags(flags));
    }

    let payload_len_u64 = read_u64(bytes, 8);
    let payload_len =
        usize::try_from(payload_len_u64).map_err(|_| StorageError::PayloadLengthOverflow)?;
    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(StorageError::PayloadTooLarge);
    }

    let payload_end = HEADER_SIZE
        .checked_add(payload_len)
        .ok_or(StorageError::PayloadLengthOverflow)?;
    if bytes.len() < payload_end {
        return Err(StorageError::TruncatedPayload);
    }
    if bytes.len() > payload_end {
        return Err(StorageError::TrailingBytes);
    }

    let expected_checksum = read_u32(bytes, 16);
    let payload = &bytes[HEADER_SIZE..payload_end];
    if crc32fast::hash(payload) != expected_checksum {
        return Err(StorageError::ChecksumMismatch);
    }

    let stored: StoredDocument =
        serde_json::from_slice(payload).map_err(|_| StorageError::InvalidJson)?;
    if stored.schema_version != SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion(
            stored.schema_version,
        ));
    }

    let snapshot = stored.into_snapshot()?;
    Document::from_snapshot(snapshot).map_err(StorageError::InvalidDocument)
}

fn wrap_payload(payload: &[u8]) -> Result<Vec<u8>, StorageError> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(StorageError::PayloadTooLarge);
    }
    let payload_len =
        u64::try_from(payload.len()).map_err(|_| StorageError::PayloadLengthOverflow)?;
    let capacity = HEADER_SIZE
        .checked_add(payload.len())
        .ok_or(StorageError::PayloadLengthOverflow)?;

    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&FILE_MAGIC);
    bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&SUPPORTED_FLAGS.to_le_bytes());
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDocument {
    schema_version: u16,
    width_px: u32,
    height_px: u32,
    next_layer_id: u64,
    next_stroke_id: u64,
    layers: Vec<StoredLayer>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLayer {
    id: u64,
    name: String,
    visible: bool,
    opacity: f32,
    strokes: Vec<StoredStroke>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredStroke {
    id: u64,
    brush: StoredBrush,
    samples: Vec<StoredSample>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredBrush {
    base_radius: f64,
    minimum_pressure_ratio: f64,
    pressure_exponent: f64,
    spacing_ratio: f64,
    max_dabs: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSample {
    x: f64,
    y: f64,
    pressure: Option<f32>,
    timestamp_ns: u64,
}

impl StoredDocument {
    fn from_snapshot(snapshot: DocumentSnapshot) -> Result<Self, StorageError> {
        let layers = snapshot
            .layers
            .into_iter()
            .map(|layer| {
                let strokes = layer
                    .strokes
                    .into_iter()
                    .map(|stroke| {
                        let brush = StoredBrush {
                            base_radius: stroke.brush.base_radius,
                            minimum_pressure_ratio: stroke.brush.minimum_pressure_ratio,
                            pressure_exponent: stroke.brush.pressure_exponent,
                            spacing_ratio: stroke.brush.spacing_ratio,
                            max_dabs: u64::try_from(stroke.brush.max_dabs)
                                .map_err(|_| StorageError::PayloadLengthOverflow)?,
                        };
                        let samples = stroke
                            .samples
                            .into_iter()
                            .map(|sample| StoredSample {
                                x: sample.x,
                                y: sample.y,
                                pressure: sample.pressure,
                                timestamp_ns: sample.timestamp_ns,
                            })
                            .collect();
                        Ok(StoredStroke {
                            id: stroke.id,
                            brush,
                            samples,
                        })
                    })
                    .collect::<Result<Vec<_>, StorageError>>()?;
                Ok(StoredLayer {
                    id: layer.id,
                    name: layer.name,
                    visible: layer.visible,
                    opacity: layer.opacity,
                    strokes,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        Ok(Self {
            schema_version: SCHEMA_VERSION,
            width_px: snapshot.width_px,
            height_px: snapshot.height_px,
            next_layer_id: snapshot.next_layer_id,
            next_stroke_id: snapshot.next_stroke_id,
            layers,
        })
    }

    fn into_snapshot(self) -> Result<DocumentSnapshot, StorageError> {
        let layers = self
            .layers
            .into_iter()
            .map(|layer| {
                let strokes = layer
                    .strokes
                    .into_iter()
                    .map(|stroke| {
                        let brush = BrushSnapshot {
                            base_radius: stroke.brush.base_radius,
                            minimum_pressure_ratio: stroke.brush.minimum_pressure_ratio,
                            pressure_exponent: stroke.brush.pressure_exponent,
                            spacing_ratio: stroke.brush.spacing_ratio,
                            max_dabs: usize::try_from(stroke.brush.max_dabs).map_err(|_| {
                                StorageError::InvalidDocument(DocumentError::InvalidBrushConfig)
                            })?,
                        };
                        let samples = stroke
                            .samples
                            .into_iter()
                            .map(|sample| StrokeSampleSnapshot {
                                x: sample.x,
                                y: sample.y,
                                pressure: sample.pressure,
                                timestamp_ns: sample.timestamp_ns,
                            })
                            .collect();
                        Ok(StrokeSnapshot {
                            id: stroke.id,
                            brush,
                            samples,
                        })
                    })
                    .collect::<Result<Vec<_>, StorageError>>()?;
                Ok(LayerSnapshot {
                    id: layer.id,
                    name: layer.name,
                    visible: layer.visible,
                    opacity: layer.opacity,
                    strokes,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        Ok(DocumentSnapshot {
            width_px: self.width_px,
            height_px: self.height_px,
            next_layer_id: self.next_layer_id,
            next_stroke_id: self.next_stroke_id,
            layers,
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;
    use rasm_engine_document::{Document, Edit, EditResult, History};
    use rasm_engine_ink::{BrushConfig, StrokeSample};

    use super::{
        FILE_MAGIC, HEADER_SIZE, MAX_PAYLOAD_BYTES, SCHEMA_VERSION, StorageError, StoredDocument,
        decode, encode, wrap_payload,
    };

    fn sample(x: f64, y: f64, pressure: Option<f32>, timestamp_ns: u64) -> StrokeSample {
        StrokeSample::new(DVec2::new(x, y), pressure, timestamp_ns).expect("valid sample")
    }

    fn populated_document() -> Document {
        let mut document = Document::new(1920, 1080).expect("document");
        let first_layer = document.layers()[0].id();
        let mut history = History::default();

        history
            .execute(&mut document, Edit::rename_layer(first_layer, "Sketch"))
            .expect("rename layer");
        history
            .execute(&mut document, Edit::set_layer_opacity(first_layer, 0.85))
            .expect("set opacity");
        history
            .execute(
                &mut document,
                Edit::add_stroke(
                    first_layer,
                    vec![
                        sample(10.25, 20.5, Some(0.2), 100),
                        sample(40.75, 60.125, Some(0.95), 250),
                    ],
                    BrushConfig::new(9.5, 0.08, 0.9, 0.25, 25_000).expect("brush"),
                ),
            )
            .expect("add stroke");

        let second_layer = match history
            .execute(&mut document, Edit::add_layer("Color"))
            .expect("add layer")
        {
            EditResult::LayerAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };
        history
            .execute(
                &mut document,
                Edit::set_layer_visibility(second_layer, false),
            )
            .expect("hide layer");
        history
            .execute(
                &mut document,
                Edit::add_stroke(
                    second_layer,
                    vec![sample(-4.0, 7.0, None, 300)],
                    BrushConfig::default(),
                ),
            )
            .expect("add second stroke");

        document
    }

    #[test]
    fn empty_document_round_trips() {
        let document = Document::new(800, 600).expect("document");
        let encoded = encode(&document).expect("encode");
        let decoded = decode(&encoded).expect("decode");
        assert_eq!(decoded, document);
    }

    #[test]
    fn layers_strokes_brushes_and_source_samples_round_trip() {
        let document = populated_document();
        let encoded = encode(&document).expect("encode");
        let decoded = decode(&encoded).expect("decode");
        assert_eq!(decoded, document);
    }

    #[test]
    fn header_contains_magic_and_version() {
        let document = Document::new(8, 8).expect("document");
        let bytes = encode(&document).expect("encode");
        assert_eq!(&bytes[0..4], &FILE_MAGIC);
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 1);
        assert!(bytes.len() > HEADER_SIZE);
    }

    #[test]
    fn short_header_is_rejected() {
        assert_eq!(decode(b"RASM"), Err(StorageError::TruncatedHeader));
    }

    #[test]
    fn invalid_magic_is_rejected() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        bytes[0] = b'X';
        assert_eq!(decode(&bytes), Err(StorageError::InvalidMagic));
    }

    #[test]
    fn unsupported_file_version_is_rejected() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(decode(&bytes), Err(StorageError::UnsupportedFileVersion(2)));
    }

    #[test]
    fn unsupported_flags_are_rejected() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        bytes[6..8].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(decode(&bytes), Err(StorageError::UnsupportedFlags(1)));
    }

    #[test]
    fn oversized_payload_is_rejected_before_length_matching() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        let oversized = u64::try_from(MAX_PAYLOAD_BYTES + 1).expect("size");
        bytes[8..16].copy_from_slice(&oversized.to_le_bytes());
        assert_eq!(decode(&bytes), Err(StorageError::PayloadTooLarge));
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        bytes.pop();
        assert_eq!(decode(&bytes), Err(StorageError::TruncatedPayload));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        bytes.push(0);
        assert_eq!(decode(&bytes), Err(StorageError::TrailingBytes));
    }

    #[test]
    fn payload_checksum_is_verified() {
        let mut bytes = encode(&Document::new(10, 10).expect("document")).expect("encode");
        bytes[HEADER_SIZE] ^= 0x01;
        assert_eq!(decode(&bytes), Err(StorageError::ChecksumMismatch));
    }

    #[test]
    fn unsupported_schema_version_is_rejected() {
        let mut stored: StoredDocument = serde_json::from_slice(
            &encode(&Document::new(10, 10).expect("document")).expect("encode")[HEADER_SIZE..],
        )
        .expect("stored document");
        stored.schema_version = SCHEMA_VERSION + 1;
        let payload = serde_json::to_vec(&stored).expect("serialize");
        let bytes = wrap_payload(&payload).expect("wrap");
        assert_eq!(
            decode(&bytes),
            Err(StorageError::UnsupportedSchemaVersion(SCHEMA_VERSION + 1))
        );
    }

    #[test]
    fn invalid_json_with_valid_checksum_is_rejected() {
        let bytes = wrap_payload(b"{not-json").expect("wrap");
        assert_eq!(decode(&bytes), Err(StorageError::InvalidJson));
    }
}

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum complete file size accepted by load, including the fixed header.
pub const MAX_FILE_BYTES: u64 = (MAX_PAYLOAD_BYTES as u64) + (HEADER_SIZE as u64);

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Errors from filesystem-backed save and load operations.
#[derive(Debug)]
pub enum FileStorageError {
    Io(io::Error),
    Codec(StorageError),
    FileTooLarge,
    /// The replacement committed, but syncing the containing directory failed.
    /// The new file may already be visible; callers must not assume the old file remains.
    DirectorySyncAfterCommit(io::Error),
}

impl fmt::Display for FileStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "filesystem operation failed: {error}"),
            Self::Codec(error) => write!(formatter, "project codec failed: {error}"),
            Self::FileTooLarge => {
                formatter.write_str("project file exceeds the configured size limit")
            }
            Self::DirectorySyncAfterCommit(error) => {
                write!(
                    formatter,
                    "file was replaced, but directory sync failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for FileStorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::DirectorySyncAfterCommit(error) => Some(error),
            Self::Codec(error) => Some(error),
            Self::FileTooLarge => None,
        }
    }
}

impl From<StorageError> for FileStorageError {
    fn from(error: StorageError) -> Self {
        Self::Codec(error)
    }
}

/// Saves a document through a same-directory temporary file and atomic replacement.
///
/// A successful return means the replacement operation completed. On Unix, a
/// directory-sync error is reported separately because the rename has already
/// committed and cannot safely be rolled back.
pub fn save_atomic(path: impl AsRef<Path>, document: &Document) -> Result<(), FileStorageError> {
    let bytes = encode(document)?;
    write_atomic_bytes(path.as_ref(), &bytes, replace_file)
}

/// Loads a project file after checking its size and validating its complete RASM payload.
pub fn load(path: impl AsRef<Path>) -> Result<Document, FileStorageError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(FileStorageError::Io)?;
    let metadata_len = file.metadata().map_err(FileStorageError::Io)?.len();
    if metadata_len > MAX_FILE_BYTES {
        return Err(FileStorageError::FileTooLarge);
    }

    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata_len).map_err(|_| FileStorageError::FileTooLarge)?,
    );
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(FileStorageError::Io)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FileStorageError::FileTooLarge);
    }
    decode(&bytes).map_err(FileStorageError::Codec)
}

fn write_atomic_bytes<F>(path: &Path, bytes: &[u8], replace: F) -> Result<(), FileStorageError>
where
    F: FnOnce(&Path, &Path) -> io::Result<()>,
{
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        FileStorageError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination path must include a file name",
        ))
    })?;

    let (temp_path, mut temp_file) =
        create_temp_file(parent, file_name).map_err(FileStorageError::Io)?;

    let write_result = (|| {
        temp_file.write_all(bytes).map_err(FileStorageError::Io)?;
        temp_file.flush().map_err(FileStorageError::Io)?;
        temp_file.sync_all().map_err(FileStorageError::Io)?;
        Ok(())
    })();
    drop(temp_file);

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = replace(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(FileStorageError::Io(error));
    }

    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(FileStorageError::DirectorySyncAfterCommit)?;

    Ok(())
}

fn create_temp_file(
    parent: &Path,
    destination_name: &std::ffi::OsStr,
) -> io::Result<(PathBuf, File)> {
    for _ in 0..128 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut name = std::ffi::OsString::from(".");
        name.push(destination_name);
        name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
        let temp_path = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary project file",
    ))
}

fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };

        let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination_wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let result = unsafe {
            MoveFileExW(
                source_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

#[cfg(test)]
mod file_tests {
    use super::{FileStorageError, load, save_atomic, write_atomic_bytes};
    use crate::encode;
    use rasm_engine_document::{Document, Edit, History};
    use std::fs;
    use std::io;
    use std::path::Path;

    fn sample_document() -> Document {
        let mut document = Document::new(640, 480).expect("document");
        let layer = document.layers()[0].id();
        let mut history = History::default();
        history
            .execute(&mut document, Edit::rename_layer(layer, "Saved"))
            .expect("rename");
        document
    }

    #[test]
    fn saves_and_loads_a_project_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("drawing.rasm");
        let document = sample_document();

        save_atomic(&path, &document).expect("save");
        assert_eq!(load(&path).expect("load"), document);
    }

    #[test]
    fn replaces_an_existing_project_without_partial_content() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("drawing.rasm");
        let first = Document::new(100, 100).expect("first");
        let second = Document::new(900, 700).expect("second");

        save_atomic(&path, &first).expect("initial save");
        save_atomic(&path, &second).expect("replace");
        assert_eq!(load(&path).expect("load"), second);
    }

    #[test]
    fn failed_commit_keeps_previous_file_and_cleans_temporary_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("drawing.rasm");
        let previous = Document::new(100, 100).expect("previous");
        let replacement = Document::new(900, 700).expect("replacement");
        let previous_bytes = encode(&previous).expect("encode");
        let replacement_bytes = encode(&replacement).expect("encode");
        fs::write(&path, &previous_bytes).expect("write previous");

        let error = write_atomic_bytes(&path, &replacement_bytes, |_source, _destination| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected commit failure",
            ))
        })
        .expect_err("commit should fail");

        assert!(matches!(error, FileStorageError::Io(_)));
        assert_eq!(fs::read(&path).expect("read previous"), previous_bytes);
        let remaining: Vec<_> = fs::read_dir(directory.path())
            .expect("list directory")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0], "drawing.rasm");
    }

    #[test]
    fn malformed_project_is_rejected_without_changing_source_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("broken.rasm");
        fs::write(&path, b"not a RASM file").expect("write");
        let before = fs::read(&path).expect("read");

        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).expect("read again"), before);
    }

    #[test]
    fn destination_must_have_a_file_name() {
        let directory = tempfile::tempdir().expect("temp dir");
        let result = write_atomic_bytes(Path::new(""), b"bytes", |_source, _destination| Ok(()));
        assert!(matches!(result, Err(FileStorageError::Io(_))));
        let _ = directory;
    }
}
