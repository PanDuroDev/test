//! Platform-independent stylus input and brush-dab generation.
//!
//! The engine preserves normalized input samples and derives a deterministic
//! dab stream from path distance rather than event arrival frequency. No GPU,
//! windowing, or platform input API is used here.

use std::fmt;

mod geometry;
pub use geometry::{
    StrokeBounds, StrokeGeometryConfig, StrokeGeometryError, StrokeMesh, StrokeVertex,
    tessellate_stroke_mesh,
};

use glam::DVec2;

/// Errors returned when validating pen input or sampling a stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeError {
    EmptyStroke,
    InvalidPosition,
    InvalidPressure,
    NonMonotonicTimestamp,
    InvalidBrushConfig,
    TooManyDabs,
}

impl fmt::Display for StrokeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyStroke => "a stroke must contain at least one sample",
            Self::InvalidPosition => "stroke position must be finite",
            Self::InvalidPressure => "pressure must be finite and normalized to [0, 1]",
            Self::NonMonotonicTimestamp => "stroke timestamps must be monotonic",
            Self::InvalidBrushConfig => "brush configuration contains invalid values",
            Self::TooManyDabs => "stroke exceeds the configured dab limit",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for StrokeError {}

/// One normalized input sample from a pointer or stylus.
///
/// A missing pressure value means the device does not provide a trustworthy
/// pressure axis. The current fallback treats such devices as full pressure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeSample {
    position: DVec2,
    pressure: Option<f32>,
    timestamp_ns: u64,
}

impl StrokeSample {
    /// Creates a validated input sample.
    /// Creates a validated sample from scalar document-space coordinates.
    pub fn from_xy(
        x: f64,
        y: f64,
        pressure: Option<f32>,
        timestamp_ns: u64,
    ) -> Result<Self, StrokeError> {
        Self::new(DVec2::new(x, y), pressure, timestamp_ns)
    }

    pub fn new(
        position: DVec2,
        pressure: Option<f32>,
        timestamp_ns: u64,
    ) -> Result<Self, StrokeError> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(StrokeError::InvalidPosition);
        }
        if pressure.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
            return Err(StrokeError::InvalidPressure);
        }
        Ok(Self {
            position,
            pressure,
            timestamp_ns,
        })
    }

    pub const fn position(self) -> DVec2 {
        self.position
    }

    pub const fn pressure(self) -> Option<f32> {
        self.pressure
    }

    pub const fn timestamp_ns(self) -> u64 {
        self.timestamp_ns
    }

    fn effective_pressure(self) -> f64 {
        f64::from(self.pressure.unwrap_or(1.0))
    }
}

/// Parameters for deterministic variable-width brush sampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushConfig {
    base_radius: f64,
    minimum_pressure_ratio: f64,
    pressure_exponent: f64,
    spacing_ratio: f64,
    max_dabs: usize,
}

impl BrushConfig {
    /// Creates a brush configuration.
    ///
    /// The spacing ratio is a fraction of the local diameter and must not
    /// exceed 0.5, so dab centres stay within the smaller endpoint radius.
    /// The minimum pressure radius must be positive to avoid zero-spacing.
    pub fn new(
        base_radius: f64,
        minimum_pressure_ratio: f64,
        pressure_exponent: f64,
        spacing_ratio: f64,
        max_dabs: usize,
    ) -> Result<Self, StrokeError> {
        let valid = base_radius.is_finite()
            && base_radius > 0.0
            && minimum_pressure_ratio.is_finite()
            && minimum_pressure_ratio > 0.0
            && minimum_pressure_ratio <= 1.0
            && pressure_exponent.is_finite()
            && pressure_exponent > 0.0
            && spacing_ratio.is_finite()
            && spacing_ratio > 0.0
            && spacing_ratio <= 0.5
            && max_dabs > 0;
        if !valid {
            return Err(StrokeError::InvalidBrushConfig);
        }
        Ok(Self {
            base_radius,
            minimum_pressure_ratio,
            pressure_exponent,
            spacing_ratio,
            max_dabs,
        })
    }

    pub const fn base_radius(self) -> f64 {
        self.base_radius
    }

    /// Returns the same brush shape with a different base radius.
    pub fn with_base_radius(self, base_radius: f64) -> Result<Self, StrokeError> {
        Self::new(
            base_radius,
            self.minimum_pressure_ratio,
            self.pressure_exponent,
            self.spacing_ratio,
            self.max_dabs,
        )
    }

    pub const fn minimum_pressure_ratio(self) -> f64 {
        self.minimum_pressure_ratio
    }

    pub const fn pressure_exponent(self) -> f64 {
        self.pressure_exponent
    }

    pub const fn spacing_ratio(self) -> f64 {
        self.spacing_ratio
    }

    pub const fn max_dabs(self) -> usize {
        self.max_dabs
    }

    /// Returns the brush radius produced by a normalized pressure value.
    pub fn radius_at_pressure(self, pressure: f64) -> f64 {
        let pressure_curve = pressure.clamp(0.0, 1.0).powf(self.pressure_exponent);
        self.base_radius
            * (self.minimum_pressure_ratio + (1.0 - self.minimum_pressure_ratio) * pressure_curve)
    }
}

impl Default for BrushConfig {
    fn default() -> Self {
        Self {
            base_radius: 8.0,
            minimum_pressure_ratio: 0.12,
            pressure_exponent: 0.75,
            spacing_ratio: 0.35,
            max_dabs: 200_000,
        }
    }
}

/// A circular brush stamp in document coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushDab {
    position: DVec2,
    radius: f64,
    timestamp_ns: u64,
}

impl BrushDab {
    pub const fn position(self) -> DVec2 {
        self.position
    }

    pub const fn radius(self) -> f64 {
        self.radius
    }

    pub const fn timestamp_ns(self) -> u64 {
        self.timestamp_ns
    }
}

/// Converts raw path samples into an ordered dab stream.
///
/// Samples are interpolated by geometric distance, not by the number or rate
/// of MotionEvent callbacks. The first and last samples are always represented.
/// Identical consecutive positions are coalesced into one dab, retaining the
/// larger radius so a stationary pressure change does not repeatedly overdraw.
pub fn tessellate_stroke(
    samples: &[StrokeSample],
    brush: BrushConfig,
) -> Result<Vec<BrushDab>, StrokeError> {
    if samples.is_empty() {
        return Err(StrokeError::EmptyStroke);
    }
    if samples
        .windows(2)
        .any(|pair| pair[1].timestamp_ns < pair[0].timestamp_ns)
    {
        return Err(StrokeError::NonMonotonicTimestamp);
    }

    let first = samples[0];
    let mut dabs = vec![BrushDab {
        position: first.position,
        radius: brush.radius_at_pressure(first.effective_pressure()),
        timestamp_ns: first.timestamp_ns,
    }];

    for pair in samples.windows(2) {
        let start = pair[0];
        let end = pair[1];
        let start_radius = brush.radius_at_pressure(start.effective_pressure());
        let end_radius = brush.radius_at_pressure(end.effective_pressure());
        let distance = start.position.distance(end.position);

        if distance <= f64::EPSILON {
            if let Some(last) = dabs.last_mut() {
                last.radius = last.radius.max(end_radius);
                last.timestamp_ns = end.timestamp_ns;
            }
            continue;
        }

        let maximum_spacing = 2.0 * start_radius.min(end_radius) * brush.spacing_ratio;
        let required_steps = (distance / maximum_spacing).ceil();
        if !required_steps.is_finite()
            || required_steps < 1.0
            || required_steps > (brush.max_dabs - dabs.len()) as f64
        {
            return Err(StrokeError::TooManyDabs);
        }

        let steps = required_steps as usize;
        if dabs
            .len()
            .checked_add(steps)
            .is_none_or(|count| count > brush.max_dabs)
        {
            return Err(StrokeError::TooManyDabs);
        }

        let start_pressure = start.effective_pressure();
        let end_pressure = end.effective_pressure();
        let elapsed_ns = u128::from(end.timestamp_ns - start.timestamp_ns);

        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            let offset_ns = elapsed_ns * step as u128 / steps as u128;
            dabs.push(BrushDab {
                position: start.position.lerp(end.position, t),
                radius: brush
                    .radius_at_pressure(start_pressure + (end_pressure - start_pressure) * t),
                timestamp_ns: start.timestamp_ns + offset_ns as u64,
            });
        }
    }

    Ok(dabs)
}

#[cfg(test)]
mod tests {
    use super::{BrushConfig, StrokeError, StrokeSample, tessellate_stroke};
    use glam::DVec2;

    fn sample(x: f64, y: f64, pressure: Option<f32>, time: u64) -> StrokeSample {
        StrokeSample::new(DVec2::new(x, y), pressure, time).expect("valid sample")
    }

    #[test]
    fn empty_stroke_is_rejected() {
        assert_eq!(
            tessellate_stroke(&[], BrushConfig::default()),
            Err(StrokeError::EmptyStroke)
        );
    }

    #[test]
    fn single_sample_produces_one_dab() {
        let dabs = tessellate_stroke(&[sample(3.0, 4.0, Some(1.0), 10)], BrushConfig::default())
            .expect("valid stroke");

        assert_eq!(dabs.len(), 1);
        assert_eq!(dabs[0].position(), DVec2::new(3.0, 4.0));
        assert_eq!(dabs[0].radius(), 8.0);
        assert_eq!(dabs[0].timestamp_ns(), 10);
    }

    #[test]
    fn pressure_changes_radius_monotonically() {
        let brush = BrushConfig::default();
        let radii = [0.0, 0.2, 0.5, 0.8, 1.0].map(|p| brush.radius_at_pressure(p));

        assert!(radii.windows(2).all(|pair| pair[1] >= pair[0]));
        assert!((radii[0] - 0.96).abs() < 1.0e-12);
        assert_eq!(radii[4], 8.0);
    }

    #[test]
    fn missing_pressure_uses_full_pressure_fallback() {
        let dabs = tessellate_stroke(&[sample(0.0, 0.0, None, 0)], BrushConfig::default())
            .expect("valid stroke");

        assert_eq!(dabs[0].radius(), 8.0);
    }

    #[test]
    fn straight_stroke_includes_endpoints_without_gaps() {
        let samples = [
            sample(0.0, 0.0, Some(0.8), 0),
            sample(100.0, 0.0, Some(0.8), 1_000_000),
        ];
        let dabs = tessellate_stroke(&samples, BrushConfig::default()).expect("valid stroke");

        assert_eq!(dabs.first().expect("first").position(), DVec2::ZERO);
        assert_eq!(
            dabs.last().expect("last").position(),
            DVec2::new(100.0, 0.0)
        );
        for pair in dabs.windows(2) {
            let spacing = pair[0].position().distance(pair[1].position());
            let allowed = pair[0].radius().min(pair[1].radius());
            assert!(spacing <= allowed + 1.0e-9);
        }
    }

    #[test]
    fn sampling_depends_on_path_distance_not_event_count() {
        let brush = BrushConfig::default();
        let sparse = [
            sample(0.0, 0.0, Some(1.0), 0),
            sample(40.0, 0.0, Some(1.0), 40),
        ];
        let dense = [
            sample(0.0, 0.0, Some(1.0), 0),
            sample(10.0, 0.0, Some(1.0), 10),
            sample(20.0, 0.0, Some(1.0), 20),
            sample(30.0, 0.0, Some(1.0), 30),
            sample(40.0, 0.0, Some(1.0), 40),
        ];
        let a = tessellate_stroke(&sparse, brush).expect("valid stroke");
        let b = tessellate_stroke(&dense, brush).expect("valid stroke");

        assert_eq!(a.len(), b.len());
        for (left, right) in a.iter().zip(b.iter()) {
            assert!(left.position().distance(right.position()) < 1.0e-9);
        }
    }

    #[test]
    fn identical_positions_coalesce_without_repeated_overdraw() {
        let samples = [
            sample(5.0, 5.0, Some(0.1), 0),
            sample(5.0, 5.0, Some(1.0), 1),
        ];
        let dabs = tessellate_stroke(&samples, BrushConfig::default()).expect("valid stroke");

        assert_eq!(dabs.len(), 1);
        assert_eq!(dabs[0].radius(), 8.0);
        assert_eq!(dabs[0].timestamp_ns(), 1);
    }

    #[test]
    fn decreasing_timestamps_are_rejected() {
        let samples = [
            sample(0.0, 0.0, Some(1.0), 100),
            sample(10.0, 0.0, Some(1.0), 99),
        ];

        assert_eq!(
            tessellate_stroke(&samples, BrushConfig::default()),
            Err(StrokeError::NonMonotonicTimestamp)
        );
    }

    #[test]
    fn invalid_input_values_are_rejected() {
        assert_eq!(
            StrokeSample::new(DVec2::new(f64::NAN, 0.0), Some(1.0), 0),
            Err(StrokeError::InvalidPosition)
        );
        assert_eq!(
            StrokeSample::new(DVec2::ZERO, Some(f32::NAN), 0),
            Err(StrokeError::InvalidPressure)
        );
        assert_eq!(
            StrokeSample::new(DVec2::ZERO, Some(1.1), 0),
            Err(StrokeError::InvalidPressure)
        );
    }

    #[test]
    fn invalid_brush_configuration_is_rejected() {
        assert_eq!(
            BrushConfig::new(0.0, 0.1, 1.0, 0.3, 100),
            Err(StrokeError::InvalidBrushConfig)
        );
        assert_eq!(
            BrushConfig::new(8.0, 0.0, 1.0, 0.3, 100),
            Err(StrokeError::InvalidBrushConfig)
        );
        assert_eq!(
            BrushConfig::new(8.0, 0.1, 1.0, 0.6, 100),
            Err(StrokeError::InvalidBrushConfig)
        );
        assert_eq!(
            BrushConfig::new(8.0, 0.1, 1.0, 0.3, 0),
            Err(StrokeError::InvalidBrushConfig)
        );
    }

    #[test]
    fn dab_count_is_bounded() {
        let brush = BrushConfig::new(1.0, 0.1, 1.0, 0.1, 3).expect("valid brush");
        let samples = [
            sample(0.0, 0.0, Some(1.0), 0),
            sample(100.0, 0.0, Some(1.0), 100),
        ];

        assert_eq!(
            tessellate_stroke(&samples, brush),
            Err(StrokeError::TooManyDabs)
        );
    }

    #[test]
    fn interpolated_timestamps_are_monotonic() {
        let samples = [
            sample(0.0, 0.0, Some(0.2), 1_000),
            sample(10.0, 0.0, Some(1.0), 2_000),
        ];
        let dabs = tessellate_stroke(&samples, BrushConfig::default()).expect("valid stroke");

        assert!(
            dabs.windows(2)
                .all(|pair| pair[1].timestamp_ns() >= pair[0].timestamp_ns())
        );
        assert_eq!(dabs.first().expect("first").timestamp_ns(), 1_000);
        assert_eq!(dabs.last().expect("last").timestamp_ns(), 2_000);
    }
}
