//! Platform-independent document state and reversible editing history.
//!
//! The document stores original stroke samples and brush settings. GPU meshes and
//! raster tiles are derived data and must remain outside this crate.

use std::collections::HashSet;
use std::fmt;

use rasm_engine_ink::{BrushConfig, StrokeSample};

/// Maximum UTF-8 byte length for a layer name.
pub const MAX_LAYER_NAME_BYTES: usize = 256;

/// Maximum input samples accepted for one stroke.
pub const MAX_STROKE_SAMPLES: usize = 1_000_000;

/// Default maximum number of reversible edits retained by History.
pub const DEFAULT_HISTORY_LIMIT: usize = 256;

/// Maximum number of layers accepted in a document snapshot.
pub const MAX_DOCUMENT_LAYERS: usize = 2_048;

/// Maximum number of strokes accepted in a document snapshot.
pub const MAX_DOCUMENT_STROKES: usize = 100_000;

/// Maximum number of source samples across all strokes in one document.
pub const MAX_DOCUMENT_TOTAL_SAMPLES: usize = 2_000_000;

/// Stable identifier for a layer within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(u64);

impl LayerId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable identifier for a stroke within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StrokeId(u64);

/// Normalized RGBA colour stored with each editable stroke.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeColor {
    rgba: [f32; 4],
}

impl StrokeColor {
    /// Default colour used by new strokes and legacy RASM files without colour data.
    pub const DEFAULT: Self = Self {
        rgba: [0.10, 0.78, 0.92, 1.0],
    };

    /// Creates a colour with finite, normalized RGBA channels.
    pub fn new(rgba: [f32; 4]) -> Result<Self, DocumentError> {
        if rgba
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            Ok(Self { rgba })
        } else {
            Err(DocumentError::InvalidStrokeColor)
        }
    }

    pub const fn rgba(self) -> [f32; 4] {
        self.rgba
    }
}

impl Default for StrokeColor {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl StrokeId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Errors produced by document and history operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentError {
    InvalidCanvasSize,
    EmptyLayerName,
    LayerNameTooLong,
    LayerNotFound,
    InvalidLayerIndex,
    CannotRemoveLastLayer,
    InvalidOpacity,
    InvalidStrokeColor,
    EmptyStroke,
    TooManyStrokeSamples,
    NonMonotonicStrokeTimestamps,
    StrokeNotFound,
    DuplicateLayerId,
    DuplicateStrokeId,
    InvalidLayerId,
    InvalidStrokeId,
    InvalidNextIdentifier,
    InvalidBrushConfig,
    InvalidStrokeSample,
    TooManyLayers,
    TooManyStrokes,
    TooManyTotalStrokeSamples,
    IdentifierExhausted,
    ZeroHistoryLimit,
    NothingToUndo,
    NothingToRedo,
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidCanvasSize => "document width and height must be greater than zero",
            Self::EmptyLayerName => "layer name cannot be empty",
            Self::LayerNameTooLong => "layer name exceeds the configured byte limit",
            Self::LayerNotFound => "the requested layer does not exist",
            Self::InvalidLayerIndex => "layer insertion or destination index is out of range",
            Self::CannotRemoveLastLayer => "a document must contain at least one layer",
            Self::InvalidOpacity => "layer opacity must be finite and within [0, 1]",
            Self::InvalidStrokeColor => "stroke colour channels must be finite and within [0, 1]",
            Self::EmptyStroke => "a stroke must contain at least one input sample",
            Self::TooManyStrokeSamples => "stroke exceeds the configured sample limit",
            Self::NonMonotonicStrokeTimestamps => "stroke timestamps must be monotonic",
            Self::StrokeNotFound => "the requested stroke does not exist in the target layer",
            Self::DuplicateLayerId => "the layer identifier already exists in the document",
            Self::DuplicateStrokeId => "the stroke identifier already exists in the document",
            Self::InvalidLayerId => "layer identifiers must be non-zero",
            Self::InvalidStrokeId => "stroke identifiers must be non-zero",
            Self::InvalidNextIdentifier => "snapshot next identifier counters are invalid",
            Self::InvalidBrushConfig => "snapshot contains an invalid brush configuration",
            Self::InvalidStrokeSample => "snapshot contains an invalid stroke sample",
            Self::TooManyLayers => "document exceeds the configured layer limit",
            Self::TooManyStrokes => "document exceeds the configured stroke limit",
            Self::TooManyTotalStrokeSamples => "document exceeds the configured total sample limit",
            Self::IdentifierExhausted => "document identifier space is exhausted",
            Self::ZeroHistoryLimit => "history limit must be greater than zero",
            Self::NothingToUndo => "there are no edits to undo",
            Self::NothingToRedo => "there are no edits to redo",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for DocumentError {}

/// A drawing layer. Layers are ordered bottom-to-top in Document::layers.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    id: LayerId,
    name: String,
    visible: bool,
    opacity: f32,
    strokes: Vec<Stroke>,
}

impl Layer {
    pub const fn id(&self) -> LayerId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    pub const fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    pub fn stroke(&self, id: StrokeId) -> Option<&Stroke> {
        self.strokes.iter().find(|stroke| stroke.id == id)
    }
}

/// An editable stroke represented by its source input samples and brush settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    id: StrokeId,
    samples: Vec<StrokeSample>,
    brush: BrushConfig,
    color: StrokeColor,
}

impl Stroke {
    pub const fn id(&self) -> StrokeId {
        self.id
    }

    pub fn samples(&self) -> &[StrokeSample] {
        &self.samples
    }

    pub const fn brush(&self) -> BrushConfig {
        self.brush
    }

    pub const fn color(&self) -> StrokeColor {
        self.color
    }
}

/// An editable drawing document.
///
/// The initial implementation stores a fixed raster canvas size. Coordinates
/// in stroke samples remain document-space f64 values. The layer vector is
/// ordered from bottom to top, and every document always has at least one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    width_px: u32,
    height_px: u32,
    layers: Vec<Layer>,
    next_layer_id: u64,
    next_stroke_id: u64,
}

impl Document {
    /// Creates a document with one visible, fully opaque layer named "Layer 1".
    pub fn new(width_px: u32, height_px: u32) -> Result<Self, DocumentError> {
        if width_px == 0 || height_px == 0 {
            return Err(DocumentError::InvalidCanvasSize);
        }

        Ok(Self {
            width_px,
            height_px,
            layers: vec![Layer {
                id: LayerId(1),
                name: "Layer 1".to_owned(),
                visible: true,
                opacity: 1.0,
                strokes: Vec::new(),
            }],
            next_layer_id: 2,
            next_stroke_id: 1,
        })
    }

    pub const fn width_px(&self) -> u32 {
        self.width_px
    }

    pub const fn height_px(&self) -> u32 {
        self.height_px
    }

    /// Returns layers in bottom-to-top compositing order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|layer| layer.id == id)
    }

    fn contains_stroke(&self, id: StrokeId) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.strokes.iter().any(|stroke| stroke.id == id))
    }

    fn total_stroke_count(&self) -> Option<usize> {
        self.layers.iter().try_fold(0_usize, |count, layer| {
            count.checked_add(layer.strokes.len())
        })
    }

    fn total_sample_count(&self) -> Option<usize> {
        self.layers.iter().try_fold(0_usize, |count, layer| {
            layer.strokes.iter().try_fold(count, |total, stroke| {
                total.checked_add(stroke.samples.len())
            })
        })
    }

    /// Exports a validated, renderer-independent snapshot of the document.
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            width_px: self.width_px,
            height_px: self.height_px,
            next_layer_id: self.next_layer_id,
            next_stroke_id: self.next_stroke_id,
            layers: self
                .layers
                .iter()
                .map(|layer| LayerSnapshot {
                    id: layer.id.get(),
                    name: layer.name.clone(),
                    visible: layer.visible,
                    opacity: layer.opacity,
                    strokes: layer
                        .strokes
                        .iter()
                        .map(|stroke| StrokeSnapshot {
                            id: stroke.id.get(),
                            color: stroke.color.rgba(),
                            brush: BrushSnapshot {
                                base_radius: stroke.brush.base_radius(),
                                minimum_pressure_ratio: stroke.brush.minimum_pressure_ratio(),
                                pressure_exponent: stroke.brush.pressure_exponent(),
                                spacing_ratio: stroke.brush.spacing_ratio(),
                                max_dabs: stroke.brush.max_dabs(),
                            },
                            samples: stroke
                                .samples
                                .iter()
                                .map(|sample| {
                                    let position = sample.position();
                                    StrokeSampleSnapshot {
                                        x: position.x,
                                        y: position.y,
                                        pressure: sample.pressure(),
                                        timestamp_ns: sample.timestamp_ns(),
                                    }
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Reconstructs a document while validating all snapshot invariants.
    ///
    /// IDs and vector ordering are preserved. The next generated IDs are set
    /// above the maximum stored ID, so newly-created objects cannot collide.
    pub fn from_snapshot(snapshot: DocumentSnapshot) -> Result<Self, DocumentError> {
        if snapshot.width_px == 0 || snapshot.height_px == 0 {
            return Err(DocumentError::InvalidCanvasSize);
        }
        if snapshot.layers.is_empty() {
            return Err(DocumentError::CannotRemoveLastLayer);
        }
        if snapshot.layers.len() > MAX_DOCUMENT_LAYERS {
            return Err(DocumentError::TooManyLayers);
        }

        let mut layer_ids = HashSet::with_capacity(snapshot.layers.len());
        let mut stroke_ids = HashSet::new();
        let mut next_layer_id = 1_u64;
        let mut next_stroke_id = 1_u64;
        let mut stroke_count = 0_usize;
        let mut total_samples = 0_usize;
        let mut layers = Vec::with_capacity(snapshot.layers.len());

        for layer in snapshot.layers {
            if layer.id == 0 {
                return Err(DocumentError::InvalidLayerId);
            }
            if !layer_ids.insert(layer.id) {
                return Err(DocumentError::DuplicateLayerId);
            }
            next_layer_id = next_layer_id.max(
                layer
                    .id
                    .checked_add(1)
                    .ok_or(DocumentError::IdentifierExhausted)?,
            );
            validate_layer_name(&layer.name)?;
            if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
                return Err(DocumentError::InvalidOpacity);
            }

            stroke_count = stroke_count
                .checked_add(layer.strokes.len())
                .ok_or(DocumentError::TooManyStrokes)?;
            if stroke_count > MAX_DOCUMENT_STROKES {
                return Err(DocumentError::TooManyStrokes);
            }

            let mut strokes = Vec::with_capacity(layer.strokes.len());
            for stroke in layer.strokes {
                if stroke.id == 0 {
                    return Err(DocumentError::InvalidStrokeId);
                }
                if !stroke_ids.insert(stroke.id) {
                    return Err(DocumentError::DuplicateStrokeId);
                }
                next_stroke_id = next_stroke_id.max(
                    stroke
                        .id
                        .checked_add(1)
                        .ok_or(DocumentError::IdentifierExhausted)?,
                );

                if stroke.samples.is_empty() {
                    return Err(DocumentError::EmptyStroke);
                }
                if stroke.samples.len() > MAX_STROKE_SAMPLES {
                    return Err(DocumentError::TooManyStrokeSamples);
                }
                total_samples = total_samples
                    .checked_add(stroke.samples.len())
                    .ok_or(DocumentError::TooManyTotalStrokeSamples)?;
                if total_samples > MAX_DOCUMENT_TOTAL_SAMPLES {
                    return Err(DocumentError::TooManyTotalStrokeSamples);
                }

                let color = StrokeColor::new(stroke.color)?;
                let brush = BrushConfig::new(
                    stroke.brush.base_radius,
                    stroke.brush.minimum_pressure_ratio,
                    stroke.brush.pressure_exponent,
                    stroke.brush.spacing_ratio,
                    stroke.brush.max_dabs,
                )
                .map_err(|_| DocumentError::InvalidBrushConfig)?;

                let mut samples = Vec::with_capacity(stroke.samples.len());
                let mut previous_timestamp = None;
                for sample in stroke.samples {
                    if previous_timestamp.is_some_and(|previous| sample.timestamp_ns < previous) {
                        return Err(DocumentError::NonMonotonicStrokeTimestamps);
                    }
                    let validated = StrokeSample::from_xy(
                        sample.x,
                        sample.y,
                        sample.pressure,
                        sample.timestamp_ns,
                    )
                    .map_err(|_| DocumentError::InvalidStrokeSample)?;
                    previous_timestamp = Some(sample.timestamp_ns);
                    samples.push(validated);
                }

                strokes.push(Stroke {
                    id: StrokeId(stroke.id),
                    samples,
                    brush,
                    color,
                });
            }

            layers.push(Layer {
                id: LayerId(layer.id),
                name: layer.name,
                visible: layer.visible,
                opacity: layer.opacity,
                strokes,
            });
        }

        if snapshot.next_layer_id < next_layer_id
            || snapshot.next_stroke_id < next_stroke_id
            || snapshot.next_layer_id == 0
            || snapshot.next_stroke_id == 0
        {
            return Err(DocumentError::InvalidNextIdentifier);
        }

        Ok(Self {
            width_px: snapshot.width_px,
            height_px: snapshot.height_px,
            layers,
            next_layer_id: snapshot.next_layer_id,
            next_stroke_id: snapshot.next_stroke_id,
        })
    }
}

/// Plain data transfer representation of a document.
///
/// Snapshot fields are public by design; callers must use
/// Document::from_snapshot to validate untrusted input before use.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSnapshot {
    pub width_px: u32,
    pub height_px: u32,
    pub next_layer_id: u64,
    pub next_stroke_id: u64,
    pub layers: Vec<LayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerSnapshot {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub strokes: Vec<StrokeSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrokeSnapshot {
    pub id: u64,
    pub color: [f32; 4],
    pub brush: BrushSnapshot,
    pub samples: Vec<StrokeSampleSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushSnapshot {
    pub base_radius: f64,
    pub minimum_pressure_ratio: f64,
    pub pressure_exponent: f64,
    pub spacing_ratio: f64,
    pub max_dabs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeSampleSnapshot {
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f32>,
    pub timestamp_ns: u64,
}

/// Result information for edits that allocate a new stable identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditResult {
    None,
    LayerAdded(LayerId),
    StrokeAdded(StrokeId),
}

/// A user edit. Use the constructors to create editing commands.
#[derive(Debug)]
pub struct Edit(Operation);

impl Edit {
    /// Adds a layer at the top of the stack.
    pub fn add_layer(name: impl Into<String>) -> Self {
        Self(Operation::AddLayer {
            name: name.into(),
            index: None,
        })
    }

    /// Inserts a layer at an explicit bottom-to-top index.
    pub fn insert_layer(name: impl Into<String>, index: usize) -> Self {
        Self(Operation::AddLayer {
            name: name.into(),
            index: Some(index),
        })
    }

    pub fn remove_layer(id: LayerId) -> Self {
        Self(Operation::RemoveLayer { id })
    }

    pub fn rename_layer(id: LayerId, name: impl Into<String>) -> Self {
        Self(Operation::RenameLayer {
            id,
            name: name.into(),
        })
    }

    pub fn set_layer_visibility(id: LayerId, visible: bool) -> Self {
        Self(Operation::SetLayerVisibility { id, visible })
    }

    pub fn set_layer_opacity(id: LayerId, opacity: f32) -> Self {
        Self(Operation::SetLayerOpacity { id, opacity })
    }

    pub fn move_layer(id: LayerId, to_index: usize) -> Self {
        Self(Operation::MoveLayer { id, to_index })
    }

    /// Appends a stroke using the legacy default colour.
    pub fn add_stroke(layer_id: LayerId, samples: Vec<StrokeSample>, brush: BrushConfig) -> Self {
        Self::add_stroke_with_color(layer_id, samples, brush, StrokeColor::DEFAULT)
    }

    /// Appends a stroke with an explicit, validated colour.
    pub fn add_stroke_with_color(
        layer_id: LayerId,
        samples: Vec<StrokeSample>,
        brush: BrushConfig,
        color: StrokeColor,
    ) -> Self {
        Self(Operation::AddStroke {
            layer_id,
            samples,
            brush,
            color,
        })
    }

    pub fn remove_stroke(layer_id: LayerId, stroke_id: StrokeId) -> Self {
        Self(Operation::RemoveStroke {
            layer_id,
            stroke_id,
        })
    }
}

/// Bounded command history with reversible undo and redo operations.
///
/// The limit is measured in command entries, not bytes. A single large stroke
/// can therefore consume substantial memory; a byte-budget policy is a
/// separate concern for a production drawing session.
#[derive(Debug)]
pub struct History {
    undo: Vec<Operation>,
    redo: Vec<Operation>,
    max_entries: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_entries: DEFAULT_HISTORY_LIMIT,
        }
    }
}

impl History {
    pub fn with_limit(max_entries: usize) -> Result<Self, DocumentError> {
        if max_entries == 0 {
            return Err(DocumentError::ZeroHistoryLimit);
        }
        Ok(Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_entries,
        })
    }

    pub const fn max_entries(&self) -> usize {
        self.max_entries
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Executes an edit atomically. Failed edits leave both document and
    /// history stacks unchanged. A successful new edit invalidates the redo
    /// branch.
    pub fn execute(
        &mut self,
        document: &mut Document,
        edit: Edit,
    ) -> Result<EditResult, DocumentError> {
        match edit.0.apply(document) {
            Ok((inverse, result)) => {
                push_bounded(&mut self.undo, inverse, self.max_entries);
                self.redo.clear();
                Ok(result)
            }
            Err((_edit, error)) => Err(error),
        }
    }

    pub fn undo(&mut self, document: &mut Document) -> Result<(), DocumentError> {
        let edit = self.undo.pop().ok_or(DocumentError::NothingToUndo)?;
        match edit.apply(document) {
            Ok((redo, _)) => {
                push_bounded(&mut self.redo, redo, self.max_entries);
                Ok(())
            }
            Err((edit, error)) => {
                self.undo.push(edit);
                Err(error)
            }
        }
    }

    pub fn redo(&mut self, document: &mut Document) -> Result<(), DocumentError> {
        let edit = self.redo.pop().ok_or(DocumentError::NothingToRedo)?;
        match edit.apply(document) {
            Ok((undo, _)) => {
                push_bounded(&mut self.undo, undo, self.max_entries);
                Ok(())
            }
            Err((edit, error)) => {
                self.redo.push(edit);
                Err(error)
            }
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

fn push_bounded(stack: &mut Vec<Operation>, operation: Operation, limit: usize) {
    if stack.len() == limit {
        stack.remove(0);
    }
    stack.push(operation);
}

fn validate_layer_name(name: &str) -> Result<(), DocumentError> {
    if name.trim().is_empty() {
        return Err(DocumentError::EmptyLayerName);
    }
    if name.len() > MAX_LAYER_NAME_BYTES {
        return Err(DocumentError::LayerNameTooLong);
    }
    Ok(())
}

#[derive(Debug)]
enum Operation {
    AddLayer {
        name: String,
        index: Option<usize>,
    },
    RemoveLayer {
        id: LayerId,
    },
    RestoreLayer {
        layer: Box<Layer>,
        index: usize,
    },
    RenameLayer {
        id: LayerId,
        name: String,
    },
    SetLayerVisibility {
        id: LayerId,
        visible: bool,
    },
    SetLayerOpacity {
        id: LayerId,
        opacity: f32,
    },
    MoveLayer {
        id: LayerId,
        to_index: usize,
    },
    AddStroke {
        layer_id: LayerId,
        samples: Vec<StrokeSample>,
        brush: BrushConfig,
        color: StrokeColor,
    },
    RemoveStroke {
        layer_id: LayerId,
        stroke_id: StrokeId,
    },
    RestoreStroke {
        layer_id: LayerId,
        stroke: Box<Stroke>,
        index: usize,
    },
}

impl Operation {
    /// On failure, returns ownership of the original operation so callers can
    /// restore it to the stack without cloning stroke payloads.
    fn apply(self, document: &mut Document) -> Result<(Self, EditResult), (Self, DocumentError)> {
        match self {
            Self::AddLayer { name, index } => {
                if document.layers.len() >= MAX_DOCUMENT_LAYERS {
                    return Err((Self::AddLayer { name, index }, DocumentError::TooManyLayers));
                }
                if let Err(error) = validate_layer_name(&name) {
                    return Err((Self::AddLayer { name, index }, error));
                }
                let index = index.unwrap_or(document.layers.len());
                if index > document.layers.len() {
                    return Err((
                        Self::AddLayer {
                            name,
                            index: Some(index),
                        },
                        DocumentError::InvalidLayerIndex,
                    ));
                }
                let Some(next_id) = document.next_layer_id.checked_add(1) else {
                    return Err((
                        Self::AddLayer {
                            name,
                            index: Some(index),
                        },
                        DocumentError::IdentifierExhausted,
                    ));
                };

                let id = LayerId(document.next_layer_id);
                let layer = Layer {
                    id,
                    name,
                    visible: true,
                    opacity: 1.0,
                    strokes: Vec::new(),
                };
                document.next_layer_id = next_id;
                document.layers.insert(index, layer);
                Ok((Self::RemoveLayer { id }, EditResult::LayerAdded(id)))
            }
            Self::RemoveLayer { id } => {
                let Some(index) = document.layer_index(id) else {
                    return Err((Self::RemoveLayer { id }, DocumentError::LayerNotFound));
                };
                if document.layers.len() == 1 {
                    return Err((
                        Self::RemoveLayer { id },
                        DocumentError::CannotRemoveLastLayer,
                    ));
                }

                let layer = Box::new(document.layers.remove(index));
                Ok((Self::RestoreLayer { layer, index }, EditResult::None))
            }
            Self::RestoreLayer { layer, index } => {
                if document.layers.len() >= MAX_DOCUMENT_LAYERS {
                    return Err((
                        Self::RestoreLayer { layer, index },
                        DocumentError::TooManyLayers,
                    ));
                }
                if index > document.layers.len() {
                    return Err((
                        Self::RestoreLayer { layer, index },
                        DocumentError::InvalidLayerIndex,
                    ));
                }
                if document.layer(layer.id).is_some() {
                    return Err((
                        Self::RestoreLayer { layer, index },
                        DocumentError::DuplicateLayerId,
                    ));
                }
                if layer
                    .strokes
                    .iter()
                    .any(|stroke| document.contains_stroke(stroke.id))
                {
                    return Err((
                        Self::RestoreLayer { layer, index },
                        DocumentError::DuplicateStrokeId,
                    ));
                }

                let existing_strokes = document.total_stroke_count().unwrap_or(usize::MAX);
                let combined_strokes = existing_strokes.checked_add(layer.strokes.len());
                if combined_strokes.is_none_or(|count| count > MAX_DOCUMENT_STROKES) {
                    return Err((
                        Self::RestoreLayer { layer, index },
                        DocumentError::TooManyStrokes,
                    ));
                }
                let layer_samples = layer.strokes.iter().try_fold(0_usize, |count, stroke| {
                    count.checked_add(stroke.samples.len())
                });
                let existing_samples = document.total_sample_count();
                let combined_samples = existing_samples
                    .zip(layer_samples)
                    .and_then(|(existing, added)| existing.checked_add(added));
                if combined_samples.is_none_or(|count| count > MAX_DOCUMENT_TOTAL_SAMPLES) {
                    return Err((
                        Self::RestoreLayer { layer, index },
                        DocumentError::TooManyTotalStrokeSamples,
                    ));
                }

                let id = layer.id;
                document.layers.insert(index, *layer);
                Ok((Self::RemoveLayer { id }, EditResult::None))
            }
            Self::RenameLayer { id, name } => {
                if let Err(error) = validate_layer_name(&name) {
                    return Err((Self::RenameLayer { id, name }, error));
                }
                let Some(layer) = document.layers.iter_mut().find(|layer| layer.id == id) else {
                    return Err((Self::RenameLayer { id, name }, DocumentError::LayerNotFound));
                };
                let old_name = std::mem::replace(&mut layer.name, name);
                Ok((Self::RenameLayer { id, name: old_name }, EditResult::None))
            }
            Self::SetLayerVisibility { id, visible } => {
                let Some(layer) = document.layers.iter_mut().find(|layer| layer.id == id) else {
                    return Err((
                        Self::SetLayerVisibility { id, visible },
                        DocumentError::LayerNotFound,
                    ));
                };
                let old_value = std::mem::replace(&mut layer.visible, visible);
                Ok((
                    Self::SetLayerVisibility {
                        id,
                        visible: old_value,
                    },
                    EditResult::None,
                ))
            }
            Self::SetLayerOpacity { id, opacity } => {
                if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                    return Err((
                        Self::SetLayerOpacity { id, opacity },
                        DocumentError::InvalidOpacity,
                    ));
                }
                let Some(layer) = document.layers.iter_mut().find(|layer| layer.id == id) else {
                    return Err((
                        Self::SetLayerOpacity { id, opacity },
                        DocumentError::LayerNotFound,
                    ));
                };
                let old_value = std::mem::replace(&mut layer.opacity, opacity);
                Ok((
                    Self::SetLayerOpacity {
                        id,
                        opacity: old_value,
                    },
                    EditResult::None,
                ))
            }
            Self::MoveLayer { id, to_index } => {
                let Some(from_index) = document.layer_index(id) else {
                    return Err((
                        Self::MoveLayer { id, to_index },
                        DocumentError::LayerNotFound,
                    ));
                };
                if to_index >= document.layers.len() {
                    return Err((
                        Self::MoveLayer { id, to_index },
                        DocumentError::InvalidLayerIndex,
                    ));
                }
                let layer = document.layers.remove(from_index);
                document.layers.insert(to_index, layer);
                Ok((
                    Self::MoveLayer {
                        id,
                        to_index: from_index,
                    },
                    EditResult::None,
                ))
            }
            Self::AddStroke {
                layer_id,
                samples,
                brush,
                color,
            } => {
                if samples.is_empty() {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::EmptyStroke,
                    ));
                }
                if samples.len() > MAX_STROKE_SAMPLES {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::TooManyStrokeSamples,
                    ));
                }
                if samples
                    .windows(2)
                    .any(|pair| pair[1].timestamp_ns() < pair[0].timestamp_ns())
                {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::NonMonotonicStrokeTimestamps,
                    ));
                }
                let Some(layer_index) = document.layer_index(layer_id) else {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::LayerNotFound,
                    ));
                };
                let total_strokes = document.total_stroke_count().unwrap_or(usize::MAX);
                if total_strokes >= MAX_DOCUMENT_STROKES {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::TooManyStrokes,
                    ));
                }
                let total_samples = document.total_sample_count().unwrap_or(usize::MAX);
                if total_samples
                    .checked_add(samples.len())
                    .is_none_or(|count| count > MAX_DOCUMENT_TOTAL_SAMPLES)
                {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::TooManyTotalStrokeSamples,
                    ));
                }
                let Some(next_id) = document.next_stroke_id.checked_add(1) else {
                    return Err((
                        Self::AddStroke {
                            layer_id,
                            samples,
                            brush,
                            color,
                        },
                        DocumentError::IdentifierExhausted,
                    ));
                };

                let id = StrokeId(document.next_stroke_id);
                document.next_stroke_id = next_id;
                document.layers[layer_index].strokes.push(Stroke {
                    id,
                    samples,
                    brush,
                    color,
                });
                Ok((
                    Self::RemoveStroke {
                        layer_id,
                        stroke_id: id,
                    },
                    EditResult::StrokeAdded(id),
                ))
            }
            Self::RemoveStroke {
                layer_id,
                stroke_id,
            } => {
                let Some(layer_index) = document.layer_index(layer_id) else {
                    return Err((
                        Self::RemoveStroke {
                            layer_id,
                            stroke_id,
                        },
                        DocumentError::LayerNotFound,
                    ));
                };
                let Some(index) = document.layers[layer_index]
                    .strokes
                    .iter()
                    .position(|stroke| stroke.id == stroke_id)
                else {
                    return Err((
                        Self::RemoveStroke {
                            layer_id,
                            stroke_id,
                        },
                        DocumentError::StrokeNotFound,
                    ));
                };

                let stroke = Box::new(document.layers[layer_index].strokes.remove(index));
                Ok((
                    Self::RestoreStroke {
                        layer_id,
                        stroke,
                        index,
                    },
                    EditResult::None,
                ))
            }
            Self::RestoreStroke {
                layer_id,
                stroke,
                index,
            } => {
                let Some(layer_index) = document.layer_index(layer_id) else {
                    return Err((
                        Self::RestoreStroke {
                            layer_id,
                            stroke,
                            index,
                        },
                        DocumentError::LayerNotFound,
                    ));
                };
                if index > document.layers[layer_index].strokes.len() {
                    return Err((
                        Self::RestoreStroke {
                            layer_id,
                            stroke,
                            index,
                        },
                        DocumentError::InvalidLayerIndex,
                    ));
                }
                if document.contains_stroke(stroke.id) {
                    return Err((
                        Self::RestoreStroke {
                            layer_id,
                            stroke,
                            index,
                        },
                        DocumentError::DuplicateStrokeId,
                    ));
                }
                let total_strokes = document.total_stroke_count().unwrap_or(usize::MAX);
                if total_strokes >= MAX_DOCUMENT_STROKES {
                    return Err((
                        Self::RestoreStroke {
                            layer_id,
                            stroke,
                            index,
                        },
                        DocumentError::TooManyStrokes,
                    ));
                }
                let total_samples = document.total_sample_count().unwrap_or(usize::MAX);
                if total_samples
                    .checked_add(stroke.samples.len())
                    .is_none_or(|count| count > MAX_DOCUMENT_TOTAL_SAMPLES)
                {
                    return Err((
                        Self::RestoreStroke {
                            layer_id,
                            stroke,
                            index,
                        },
                        DocumentError::TooManyTotalStrokeSamples,
                    ));
                }

                let stroke_id = stroke.id;
                document.layers[layer_index].strokes.insert(index, *stroke);
                Ok((
                    Self::RemoveStroke {
                        layer_id,
                        stroke_id,
                    },
                    EditResult::None,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;
    use rasm_engine_ink::{BrushConfig, StrokeSample};

    use super::{
        DEFAULT_HISTORY_LIMIT, Document, DocumentError, Edit, EditResult, History, Layer, LayerId,
        MAX_LAYER_NAME_BYTES, MAX_STROKE_SAMPLES, Stroke, StrokeId,
    };

    fn document() -> Document {
        Document::new(1920, 1080).expect("valid document")
    }

    fn sample(x: f64, y: f64, pressure: Option<f32>, timestamp_ns: u64) -> StrokeSample {
        StrokeSample::new(DVec2::new(x, y), pressure, timestamp_ns).expect("valid sample")
    }

    fn add_layer(history: &mut History, document: &mut Document, name: &str) -> LayerId {
        match history
            .execute(document, Edit::add_layer(name))
            .expect("add layer")
        {
            EditResult::LayerAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        }
    }

    #[test]
    fn document_requires_nonzero_dimensions() {
        assert_eq!(
            Document::new(0, 1080),
            Err(DocumentError::InvalidCanvasSize)
        );
        assert_eq!(
            Document::new(1920, 0),
            Err(DocumentError::InvalidCanvasSize)
        );
    }

    #[test]
    fn new_document_has_one_visible_opaque_layer() {
        let document = document();
        assert_eq!(document.width_px(), 1920);
        assert_eq!(document.height_px(), 1080);
        assert_eq!(document.layers().len(), 1);
        assert_eq!(document.layers()[0].id(), LayerId(1));
        assert_eq!(document.layers()[0].name(), "Layer 1");
        assert!(document.layers()[0].is_visible());
        assert_eq!(document.layers()[0].opacity(), 1.0);
    }

    #[test]
    fn add_layer_undo_redo_preserves_its_identifier_and_order() {
        let mut document = document();
        let mut history = History::default();
        let base_id = document.layers()[0].id();

        let added_id = add_layer(&mut history, &mut document, "Ink");
        assert_eq!(document.layers().len(), 2);
        assert_eq!(document.layers()[1].id(), added_id);
        assert_eq!(history.undo_len(), 1);

        history.undo(&mut document).expect("undo add layer");
        assert_eq!(document.layers().len(), 1);
        assert_eq!(document.layers()[0].id(), base_id);

        history.redo(&mut document).expect("redo add layer");
        assert_eq!(document.layers().len(), 2);
        assert_eq!(document.layers()[1].id(), added_id);
    }

    #[test]
    fn stroke_colour_is_validated_and_survives_snapshot_and_history() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        let colour = StrokeColor::new([0.2, 0.35, 0.7, 0.85]).expect("valid colour");
        let samples = vec![sample(10.0, 20.0, Some(0.5), 1)];

        history
            .execute(
                &mut document,
                Edit::add_stroke_with_color(layer_id, samples, BrushConfig::default(), colour),
            )
            .expect("add coloured stroke");

        assert_eq!(document.layers()[0].strokes()[0].color(), colour);
        let snapshot = document.snapshot();
        assert_eq!(snapshot.layers[0].strokes[0].color, [0.2, 0.35, 0.7, 0.85]);
        let restored = Document::from_snapshot(snapshot).expect("restore colour snapshot");
        assert_eq!(restored.layers()[0].strokes()[0].color(), colour);

        history.undo(&mut document).expect("undo coloured stroke");
        assert!(document.layers()[0].strokes().is_empty());
        history.redo(&mut document).expect("redo coloured stroke");
        assert_eq!(document.layers()[0].strokes()[0].color(), colour);
    }

    #[test]
    fn invalid_stroke_colour_channels_are_rejected() {
        assert_eq!(
            StrokeColor::new([0.0, f32::NAN, 0.0, 1.0]),
            Err(DocumentError::InvalidStrokeColor)
        );
        assert_eq!(
            StrokeColor::new([0.0, 0.0, 1.1, 1.0]),
            Err(DocumentError::InvalidStrokeColor)
        );

        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        history
            .execute(
                &mut document,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(1.0, 1.0, None, 1)],
                    BrushConfig::default(),
                ),
            )
            .expect("add default-colour stroke");
        let mut snapshot = document.snapshot();
        snapshot.layers[0].strokes[0].color[0] = f32::INFINITY;
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::InvalidStrokeColor)
        );
    }

    #[test]
    fn last_layer_cannot_be_removed() {
        let mut document = document();
        let mut history = History::default();
        let id = document.layers()[0].id();

        assert_eq!(
            history.execute(&mut document, Edit::remove_layer(id)),
            Err(DocumentError::CannotRemoveLastLayer)
        );
        assert_eq!(document.layers().len(), 1);
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn layer_properties_support_undo_and_redo() {
        let mut document = document();
        let mut history = History::default();
        let id = document.layers()[0].id();

        history
            .execute(&mut document, Edit::rename_layer(id, "Sketch"))
            .expect("rename");
        history
            .execute(&mut document, Edit::set_layer_visibility(id, false))
            .expect("hide");
        history
            .execute(&mut document, Edit::set_layer_opacity(id, 0.4))
            .expect("opacity");
        assert_eq!(document.layer(id).expect("layer").name(), "Sketch");
        assert!(!document.layer(id).expect("layer").is_visible());
        assert_eq!(document.layer(id).expect("layer").opacity(), 0.4);

        history.undo(&mut document).expect("undo opacity");
        history.undo(&mut document).expect("undo visibility");
        history.undo(&mut document).expect("undo name");
        assert_eq!(document.layer(id).expect("layer").name(), "Layer 1");
        assert!(document.layer(id).expect("layer").is_visible());
        assert_eq!(document.layer(id).expect("layer").opacity(), 1.0);

        history.redo(&mut document).expect("redo name");
        history.redo(&mut document).expect("redo visibility");
        history.redo(&mut document).expect("redo opacity");
        assert_eq!(document.layer(id).expect("layer").name(), "Sketch");
        assert!(!document.layer(id).expect("layer").is_visible());
        assert_eq!(document.layer(id).expect("layer").opacity(), 0.4);
    }

    #[test]
    fn layer_reordering_is_reversible() {
        let mut document = document();
        let mut history = History::default();
        let first = document.layers()[0].id();
        let second = add_layer(&mut history, &mut document, "Second");
        let third = add_layer(&mut history, &mut document, "Third");

        history
            .execute(&mut document, Edit::move_layer(first, 2))
            .expect("move layer");
        assert_eq!(
            document
                .layers()
                .iter()
                .map(Layer::id)
                .map(LayerId::get)
                .collect::<Vec<_>>(),
            vec![second.get(), third.get(), first.get()]
        );

        history.undo(&mut document).expect("undo move");
        assert_eq!(
            document
                .layers()
                .iter()
                .map(Layer::id)
                .map(LayerId::get)
                .collect::<Vec<_>>(),
            vec![first.get(), second.get(), third.get()]
        );
    }

    #[test]
    fn invalid_layer_commands_do_not_change_document_or_history() {
        let mut document = document();
        let mut history = History::default();
        let id = document.layers()[0].id();
        let before = document.clone();

        assert_eq!(
            history.execute(&mut document, Edit::rename_layer(id, "  ")),
            Err(DocumentError::EmptyLayerName)
        );
        assert_eq!(
            history.execute(&mut document, Edit::set_layer_opacity(id, f32::NAN)),
            Err(DocumentError::InvalidOpacity)
        );
        assert_eq!(
            history.execute(&mut document, Edit::insert_layer("Bad index", 2)),
            Err(DocumentError::InvalidLayerIndex)
        );

        assert_eq!(document, before);
        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn layer_name_byte_limit_is_enforced() {
        let mut document = document();
        let mut history = History::default();
        let name = "x".repeat(MAX_LAYER_NAME_BYTES + 1);

        assert_eq!(
            history.execute(&mut document, Edit::add_layer(name)),
            Err(DocumentError::LayerNameTooLong)
        );
        assert_eq!(document.layers().len(), 1);
    }

    #[test]
    fn removed_layer_is_restored_with_all_strokes_and_same_position() {
        let mut document = document();
        let mut history = History::default();
        let base_id = document.layers()[0].id();
        let layer_id = add_layer(&mut history, &mut document, "Paint");
        let samples = vec![sample(1.0, 2.0, Some(0.5), 10)];
        let stroke_id = match history
            .execute(
                &mut document,
                Edit::add_stroke(layer_id, samples.clone(), BrushConfig::default()),
            )
            .expect("add stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };

        history
            .execute(&mut document, Edit::remove_layer(layer_id))
            .expect("remove layer");
        assert!(document.layer(layer_id).is_none());

        history.undo(&mut document).expect("restore layer");
        let restored = document.layer(layer_id).expect("restored layer");
        assert_eq!(restored.strokes().len(), 1);
        assert_eq!(restored.strokes()[0].id(), stroke_id);
        assert_eq!(restored.strokes()[0].samples(), samples);
        assert_eq!(document.layers()[0].id(), base_id);
        assert_eq!(document.layers()[1].id(), layer_id);
    }

    #[test]
    fn stroke_add_undo_redo_preserves_samples_brush_and_id() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        let samples = vec![
            sample(1.0, 2.0, Some(0.25), 10),
            sample(5.0, 7.0, Some(0.9), 20),
        ];
        let brush = BrushConfig::new(12.0, 0.15, 0.8, 0.3, 500).expect("brush");

        let stroke_id = match history
            .execute(
                &mut document,
                Edit::add_stroke(layer_id, samples.clone(), brush),
            )
            .expect("add stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };

        history.undo(&mut document).expect("undo stroke");
        assert!(document.layers()[0].strokes().is_empty());

        history.redo(&mut document).expect("redo stroke");
        let stroke = document.layers()[0]
            .stroke(stroke_id)
            .expect("restored stroke");
        assert_eq!(stroke.samples(), samples);
        assert_eq!(stroke.brush(), brush);
    }

    #[test]
    fn removing_a_stroke_undoes_at_its_original_position() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();

        let add = |x: f64, history: &mut History, document: &mut Document| match history
            .execute(
                document,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(x, 0.0, Some(1.0), x as u64)],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };
        let first = add(1.0, &mut history, &mut document);
        let second = add(2.0, &mut history, &mut document);
        let third = add(3.0, &mut history, &mut document);

        history
            .execute(&mut document, Edit::remove_stroke(layer_id, second))
            .expect("remove middle stroke");
        assert_eq!(
            document.layers()[0]
                .strokes()
                .iter()
                .map(Stroke::id)
                .collect::<Vec<_>>(),
            vec![first, third]
        );

        history.undo(&mut document).expect("undo remove");
        assert_eq!(
            document.layers()[0]
                .strokes()
                .iter()
                .map(Stroke::id)
                .collect::<Vec<_>>(),
            vec![first, second, third]
        );
    }

    #[test]
    fn invalid_strokes_are_rejected_without_mutation() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        let before = document.clone();

        assert_eq!(
            history.execute(
                &mut document,
                Edit::add_stroke(layer_id, Vec::new(), BrushConfig::default()),
            ),
            Err(DocumentError::EmptyStroke)
        );
        assert_eq!(
            history.execute(
                &mut document,
                Edit::add_stroke(
                    layer_id,
                    vec![
                        sample(0.0, 0.0, Some(1.0), 10),
                        sample(2.0, 2.0, Some(1.0), 9),
                    ],
                    BrushConfig::default(),
                ),
            ),
            Err(DocumentError::NonMonotonicStrokeTimestamps)
        );

        assert_eq!(document, before);
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn missing_layer_and_stroke_are_reported_without_partial_changes() {
        let mut document = document();
        let mut history = History::default();
        let missing_layer = LayerId(999);
        let missing_stroke = StrokeId(999);
        let existing_layer = document.layers()[0].id();

        assert_eq!(
            history.execute(
                &mut document,
                Edit::add_stroke(
                    missing_layer,
                    vec![sample(0.0, 0.0, None, 0)],
                    BrushConfig::default(),
                ),
            ),
            Err(DocumentError::LayerNotFound)
        );
        assert_eq!(
            history.execute(
                &mut document,
                Edit::remove_stroke(existing_layer, missing_stroke),
            ),
            Err(DocumentError::StrokeNotFound)
        );
        assert_eq!(document.layers()[0].strokes().len(), 0);
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn bounded_history_discards_oldest_entries() {
        let mut document = document();
        let mut history = History::with_limit(2).expect("history");
        let _first = add_layer(&mut history, &mut document, "First");
        let _second = add_layer(&mut history, &mut document, "Second");
        let _third = add_layer(&mut history, &mut document, "Third");

        assert_eq!(history.undo_len(), 2);
        history.undo(&mut document).expect("undo third");
        history.undo(&mut document).expect("undo second");
        assert_eq!(document.layers().len(), 2);
        assert_eq!(document.layers()[1].name(), "First");
        assert_eq!(
            history.undo(&mut document),
            Err(DocumentError::NothingToUndo)
        );
    }

    #[test]
    fn successful_new_edit_clears_redo_but_failed_edit_does_not() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = add_layer(&mut history, &mut document, "Ink");
        history.undo(&mut document).expect("undo add");
        assert_eq!(history.redo_len(), 1);

        assert_eq!(
            history.execute(&mut document, Edit::rename_layer(LayerId(999), "Missing",)),
            Err(DocumentError::LayerNotFound)
        );
        assert_eq!(history.redo_len(), 1);

        add_layer(&mut history, &mut document, "Pencil");
        assert_eq!(history.redo_len(), 0);
        assert!(document.layer(layer_id).is_none());
    }

    #[test]
    fn history_rejects_zero_limit_and_empty_undo_redo() {
        assert_eq!(
            History::with_limit(0).err(),
            Some(DocumentError::ZeroHistoryLimit)
        );

        let mut document = document();
        let mut history = History::default();
        assert_eq!(
            history.undo(&mut document),
            Err(DocumentError::NothingToUndo)
        );
        assert_eq!(
            history.redo(&mut document),
            Err(DocumentError::NothingToRedo)
        );
        assert_eq!(history.max_entries(), DEFAULT_HISTORY_LIMIT);
    }

    #[test]
    fn sample_limit_is_enforced() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        let samples = vec![sample(0.0, 0.0, None, 0); MAX_STROKE_SAMPLES + 1];

        assert_eq!(
            history.execute(
                &mut document,
                Edit::add_stroke(layer_id, samples, BrushConfig::default()),
            ),
            Err(DocumentError::TooManyStrokeSamples)
        );
        assert!(document.layers()[0].strokes().is_empty());
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn snapshot_round_trip_preserves_document_state_and_identifiers() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = add_layer(&mut history, &mut document, "Ink");
        let stroke_id = match history
            .execute(
                &mut document,
                Edit::add_stroke(
                    layer_id,
                    vec![
                        sample(1.5, 2.5, Some(0.4), 10),
                        sample(8.5, 9.5, Some(0.9), 20),
                    ],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };

        let restored = Document::from_snapshot(document.snapshot()).expect("restore snapshot");
        assert_eq!(restored, document);
        assert_eq!(
            restored
                .layer(layer_id)
                .expect("layer")
                .stroke(stroke_id)
                .expect("stroke")
                .samples(),
            document
                .layer(layer_id)
                .expect("layer")
                .stroke(stroke_id)
                .expect("stroke")
                .samples()
        );
    }

    #[test]
    fn snapshot_restore_advances_identifiers_without_collisions() {
        let mut source = document();
        let mut history = History::default();
        let layer_id = add_layer(&mut history, &mut source, "Existing");
        let _stroke_id = match history
            .execute(
                &mut source,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(0.0, 0.0, None, 1)],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };

        let mut restored = Document::from_snapshot(source.snapshot()).expect("restore");
        let mut restored_history = History::default();
        let next_layer = add_layer(&mut restored_history, &mut restored, "Next");
        assert!(next_layer.get() > layer_id.get());
    }

    #[test]
    fn snapshot_rejects_empty_layer_list() {
        let mut snapshot = document().snapshot();
        snapshot.layers.clear();
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::CannotRemoveLastLayer)
        );
    }

    #[test]
    fn snapshot_rejects_duplicate_or_zero_layer_ids() {
        let mut snapshot = document().snapshot();
        let first = snapshot.layers[0].clone();
        snapshot.layers.push(first);
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::DuplicateLayerId)
        );

        let mut snapshot = document().snapshot();
        snapshot.layers[0].id = 0;
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::InvalidLayerId)
        );
    }

    #[test]
    fn snapshot_rejects_invalid_layer_opacity() {
        let mut snapshot = document().snapshot();
        snapshot.layers[0].opacity = f32::NAN;
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::InvalidOpacity)
        );
    }

    #[test]
    fn snapshot_rejects_invalid_brush_parameters() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        history
            .execute(
                &mut document,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(0.0, 0.0, None, 0)],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke");

        let mut snapshot = document.snapshot();
        snapshot.layers[0].strokes[0].brush.base_radius = 0.0;
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::InvalidBrushConfig)
        );
    }

    #[test]
    fn snapshot_rejects_invalid_samples_and_non_monotonic_timestamps() {
        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        history
            .execute(
                &mut document,
                Edit::add_stroke(
                    layer_id,
                    vec![
                        sample(0.0, 0.0, Some(1.0), 10),
                        sample(1.0, 1.0, Some(1.0), 20),
                    ],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke");

        let mut invalid_position = document.snapshot();
        invalid_position.layers[0].strokes[0].samples[0].x = f64::NAN;
        assert_eq!(
            Document::from_snapshot(invalid_position),
            Err(DocumentError::InvalidStrokeSample)
        );

        let mut invalid_timestamps = document.snapshot();
        invalid_timestamps.layers[0].strokes[0].samples[1].timestamp_ns = 5;
        assert_eq!(
            Document::from_snapshot(invalid_timestamps),
            Err(DocumentError::NonMonotonicStrokeTimestamps)
        );
    }

    #[test]
    fn snapshot_preserves_next_layer_id_after_a_layer_is_removed() {
        let mut source = document();
        let mut history = History::default();
        let removed = add_layer(&mut history, &mut source, "Temporary");
        history
            .execute(&mut source, Edit::remove_layer(removed))
            .expect("remove layer");

        let mut restored = Document::from_snapshot(source.snapshot()).expect("restore snapshot");
        let mut restored_history = History::default();
        let next = add_layer(&mut restored_history, &mut restored, "Next");
        assert!(next.get() > removed.get());
    }

    #[test]
    fn snapshot_preserves_next_stroke_id_after_a_stroke_is_removed() {
        let mut source = document();
        let mut history = History::default();
        let layer_id = source.layers()[0].id();
        let removed = match history
            .execute(
                &mut source,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(1.0, 2.0, None, 10)],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };
        history
            .execute(&mut source, Edit::remove_stroke(layer_id, removed))
            .expect("remove stroke");

        let mut restored = Document::from_snapshot(source.snapshot()).expect("restore snapshot");
        let mut restored_history = History::default();
        let next = match restored_history
            .execute(
                &mut restored,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(3.0, 4.0, None, 20)],
                    BrushConfig::default(),
                ),
            )
            .expect("add next stroke")
        {
            EditResult::StrokeAdded(id) => id,
            result => panic!("unexpected result: {result:?}"),
        };
        assert!(next.get() > removed.get());
    }

    #[test]
    fn snapshot_rejects_next_id_counters_below_existing_ids() {
        let mut snapshot = document().snapshot();
        snapshot.next_layer_id = 1;
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::InvalidNextIdentifier)
        );

        let mut document = document();
        let mut history = History::default();
        let layer_id = document.layers()[0].id();
        history
            .execute(
                &mut document,
                Edit::add_stroke(
                    layer_id,
                    vec![sample(0.0, 0.0, None, 0)],
                    BrushConfig::default(),
                ),
            )
            .expect("add stroke");
        let mut snapshot = document.snapshot();
        snapshot.next_stroke_id = 1;
        assert_eq!(
            Document::from_snapshot(snapshot),
            Err(DocumentError::InvalidNextIdentifier)
        );
    }
}
