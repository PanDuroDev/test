//! A deliberately small, renderer-independent Kotlin/Rust boundary.
//!
//! Only primitive DTOs cross this boundary. Internal glam, brush, mesh, tile,
//! and renderer types remain inside the Rust workspace.

use glam::DVec2;
use rasm_engine_ink::{BrushConfig, StrokeSample, tessellate_stroke, tessellate_stroke_mesh};
use rasm_engine_tiles::{DocumentRect, TileCoord, TileError, TileGrid};

uniffi::setup_scaffolding!();

/// Hard upper bound on the number of tile coordinates returned by one call.
pub const MAX_TILES_PER_STROKE: usize = 16_384;

/// One input sample received from the platform layer.
///
/// A negative timestamp is rejected before it reaches the engine's monotonic
/// nanosecond timestamp type.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StrokeSampleInput {
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f32>,
    pub timestamp_ns: i64,
}

/// Axis-aligned geometry bounds in document coordinates.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StrokeBoundsOutput {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// Tile coordinate in deterministic row-major coverage order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TileCoordinateOutput {
    pub x: i64,
    pub y: i64,
}

/// A validated stroke preview summary suitable for a Kotlin caller.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StrokePreviewOutput {
    pub bounds: StrokeBoundsOutput,
    pub tiles: Vec<TileCoordinateOutput>,
}

/// Errors exposed by the narrow platform boundary.
#[derive(Debug, uniffi::Error)]
pub enum EngineBridgeError {
    InvalidInput { message: String },
    TileCoverageExceeded,
}

impl std::fmt::Display for EngineBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput { message } => formatter.write_str(message),
            Self::TileCoverageExceeded => {
                formatter.write_str("stroke exceeds the configured tile coverage limit")
            }
        }
    }
}

impl EngineBridgeError {
    fn invalid_input(message: impl ToString) -> Self {
        Self::InvalidInput {
            message: message.to_string(),
        }
    }
}

/// Returns the FFI contract version, independent from the application version.
#[uniffi::export]
pub fn bridge_version() -> String {
    "rasm-engine-ffi/1".to_owned()
}

/// Builds a bounded stroke preview and maps its mesh bounds to dirty tiles.
///
/// The API intentionally uses scalar DTOs rather than exposing internal Rust
/// types. The tile size is expressed in document raster pixels. The returned
/// bounds do not include an implicit anti-alias margin; callers must pass the
/// required margin explicitly.
///
/// # Errors
///
/// Returns `InvalidInput` for invalid stroke samples, timestamps, tile sizes,
/// or margins. Returns `TileCoverageExceeded` when the generated coverage
/// exceeds `MAX_TILES_PER_STROKE`.
#[uniffi::export]
pub fn preview_stroke(
    samples: Vec<StrokeSampleInput>,
    tile_size: i32,
    margin: f64,
) -> Result<StrokePreviewOutput, EngineBridgeError> {
    if samples.is_empty() {
        return Err(EngineBridgeError::invalid_input("stroke has no samples"));
    }

    let tile_size = u32::try_from(tile_size)
        .ok()
        .filter(|size| *size > 0)
        .ok_or_else(|| EngineBridgeError::invalid_input("tile size must be positive"))?;

    if !margin.is_finite() || margin < 0.0 {
        return Err(EngineBridgeError::invalid_input(
            "tile margin must be finite and non-negative",
        ));
    }

    let engine_samples = samples
        .into_iter()
        .map(|sample| {
            let timestamp_ns = u64::try_from(sample.timestamp_ns)
                .map_err(|_| EngineBridgeError::invalid_input("timestamp must be non-negative"))?;

            StrokeSample::new(
                DVec2::new(sample.x, sample.y),
                sample.pressure,
                timestamp_ns,
            )
            .map_err(EngineBridgeError::invalid_input)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let dabs = tessellate_stroke(&engine_samples, BrushConfig::default())
        .map_err(EngineBridgeError::invalid_input)?;
    let mesh = tessellate_stroke_mesh(&dabs, Default::default())
        .map_err(EngineBridgeError::invalid_input)?;
    let bounds = mesh
        .bounds()
        .ok_or_else(|| EngineBridgeError::invalid_input("stroke generated empty geometry"))?;

    let rect =
        DocumentRect::new(bounds.min(), bounds.max()).map_err(EngineBridgeError::invalid_input)?;
    let grid =
        TileGrid::new(tile_size, MAX_TILES_PER_STROKE).map_err(EngineBridgeError::invalid_input)?;

    let coverage = grid.coverage_with_margin(rect, margin).map_err(|error| {
        if error == TileError::TooManyTiles {
            EngineBridgeError::TileCoverageExceeded
        } else {
            EngineBridgeError::invalid_input(error)
        }
    })?;

    let tiles = coverage.iter().map(tile_coordinate_output).collect();

    Ok(StrokePreviewOutput {
        bounds: StrokeBoundsOutput {
            min_x: bounds.min().x,
            min_y: bounds.min().y,
            max_x: bounds.max().x,
            max_y: bounds.max().y,
        },
        tiles,
    })
}

/// Converts a tile coordinate to the public FFI DTO.
fn tile_coordinate_output(tile: TileCoord) -> TileCoordinateOutput {
    TileCoordinateOutput {
        x: tile.x(),
        y: tile.y(),
    }
}

#[cfg(test)]
mod tests {
    use super::{EngineBridgeError, StrokeSampleInput, bridge_version, preview_stroke};

    fn sample(x: f64, y: f64, timestamp_ns: i64) -> StrokeSampleInput {
        StrokeSampleInput {
            x,
            y,
            pressure: Some(1.0),
            timestamp_ns,
        }
    }

    #[test]
    fn bridge_reports_its_contract_version() {
        assert_eq!(bridge_version(), "rasm-engine-ffi/1");
    }

    #[test]
    fn valid_stroke_returns_bounds_and_bounded_tile_coverage() {
        let preview = preview_stroke(
            vec![sample(-20.0, 2.0, 0), sample(20.0, 2.0, 1_000_000)],
            64,
            2.0,
        )
        .expect("valid stroke preview");

        assert!(preview.bounds.min_x < -27.0);
        assert!(preview.bounds.max_x > 27.0);
        assert!(!preview.tiles.is_empty());
        assert!(preview.tiles.len() <= super::MAX_TILES_PER_STROKE);
        assert!(
            preview
                .tiles
                .windows(2)
                .all(|pair| { (pair[0].y, pair[0].x) <= (pair[1].y, pair[1].x) })
        );
    }

    #[test]
    fn negative_timestamps_are_rejected() {
        let result = preview_stroke(vec![sample(0.0, 0.0, -1)], 64, 0.0);
        assert!(matches!(
            result,
            Err(EngineBridgeError::InvalidInput { .. })
        ));
    }

    #[test]
    fn invalid_tile_parameters_are_rejected() {
        assert!(matches!(
            preview_stroke(vec![sample(0.0, 0.0, 0)], 0, 0.0),
            Err(EngineBridgeError::InvalidInput { .. })
        ));
        assert!(matches!(
            preview_stroke(vec![sample(0.0, 0.0, 0)], 64, f64::NAN),
            Err(EngineBridgeError::InvalidInput { .. })
        ));
    }

    #[test]
    fn excessive_tile_coverage_is_rejected_before_returning_a_large_list() {
        let result = preview_stroke(
            vec![sample(0.0, 0.0, 0), sample(16_384.0, 16_384.0, 1_000_000)],
            64,
            0.0,
        );

        assert!(matches!(
            result,
            Err(EngineBridgeError::TileCoverageExceeded)
        ));
    }
}
