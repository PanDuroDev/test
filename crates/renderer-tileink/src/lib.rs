//! Experimental desktop-only adapter from Rasm brush dabs to Tileink.
//!
//! Tileink is not currently validated by its maintainers on Android or iOS.
//! Keep this adapter at the edge of the engine so a mobile renderer can be
//! integrated without changing stored input samples or brush calculations.

use std::fmt;

use kurbo::Circle;
use peniko::Color;
use rasm_engine_ink::BrushDab;
use tileink::Canvas;

/// Errors produced while constructing a Tileink canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasBuildError {
    ZeroDimensions,
    InvalidScale,
}

impl fmt::Display for CanvasBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroDimensions => "canvas dimensions must be non-zero",
            Self::InvalidScale => "canvas scale factor must be finite and positive",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CanvasBuildError {}

/// Builds a Tileink canvas containing one analytic circle per brush dab.
///
/// The caller owns the renderer and its thread affinity. Tileink's Canvas and
/// renderer types are not Send/Sync, so they must remain on the render thread
/// on which they are created.
pub fn canvas_from_dabs(
    width: u32,
    height: u32,
    scale_factor: f32,
    dabs: &[BrushDab],
    color: Color,
) -> Result<Canvas, CanvasBuildError> {
    if width == 0 || height == 0 {
        return Err(CanvasBuildError::ZeroDimensions);
    }
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(CanvasBuildError::InvalidScale);
    }

    let mut canvas = Canvas::new(width, height, scale_factor);
    for dab in dabs {
        let position = dab.position();
        canvas.push_circle(Circle::new((position.x, position.y), dab.radius()), color);
    }

    Ok(canvas)
}

#[cfg(test)]
mod tests {
    use super::{CanvasBuildError, canvas_from_dabs};
    use glam::DVec2;
    use peniko::Color;
    use rasm_engine_ink::{BrushConfig, StrokeSample, tessellate_stroke};

    #[test]
    fn converts_each_dab_into_one_canvas_draw() {
        let samples = [
            StrokeSample::new(DVec2::new(4.0, 8.0), Some(0.5), 0).expect("valid sample"),
            StrokeSample::new(DVec2::new(28.0, 8.0), Some(1.0), 100).expect("valid sample"),
        ];
        let dabs = tessellate_stroke(&samples, BrushConfig::default()).expect("valid stroke");
        let canvas = canvas_from_dabs(64, 64, 1.0, &dabs, Color::from_rgb8(20, 160, 230))
            .expect("valid canvas");

        assert_eq!(canvas.draw_count(), dabs.len());
        assert_eq!(canvas.logical_size(), (64, 64));
    }

    #[test]
    fn rejects_zero_sized_targets() {
        assert_eq!(
            canvas_from_dabs(0, 64, 1.0, &[], Color::WHITE).err(),
            Some(CanvasBuildError::ZeroDimensions)
        );
    }

    #[test]
    fn rejects_invalid_scale() {
        assert_eq!(
            canvas_from_dabs(64, 64, f32::NAN, &[], Color::WHITE).err(),
            Some(CanvasBuildError::InvalidScale)
        );
    }

    #[test]
    fn allows_a_valid_empty_canvas() {
        let canvas = canvas_from_dabs(64, 64, 1.0, &[], Color::WHITE).expect("valid empty canvas");
        assert_eq!(canvas.draw_count(), 0);
    }
}
