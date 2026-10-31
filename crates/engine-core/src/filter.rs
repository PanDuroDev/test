//! Adaptive low-pass filtering for stylus and pointer input.
//!
//! The One Euro filter raises its cutoff as signal speed increases. This reduces
//! stationary jitter without imposing the same amount of lag on fast strokes.

use std::f64::consts::TAU;

/// Configuration and state for a scalar One Euro filter.
#[derive(Debug, Clone)]
pub struct OneEuroFilter {
    min_cutoff_hz: f64,
    beta: f64,
    derivative_cutoff_hz: f64,
    previous_raw: Option<f64>,
    previous_filtered: Option<f64>,
    previous_derivative: f64,
}

/// Errors produced by invalid filter configuration or samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterError {
    InvalidMinCutoff,
    InvalidBeta,
    InvalidDerivativeCutoff,
    InvalidDeltaTime,
    NonFiniteSample,
}

impl OneEuroFilter {
    /// Creates a filter. Cutoffs must be positive and beta must be non-negative.
    pub fn new(
        min_cutoff_hz: f64,
        beta: f64,
        derivative_cutoff_hz: f64,
    ) -> Result<Self, FilterError> {
        if !min_cutoff_hz.is_finite() || min_cutoff_hz <= 0.0 {
            return Err(FilterError::InvalidMinCutoff);
        }
        if !beta.is_finite() || beta < 0.0 {
            return Err(FilterError::InvalidBeta);
        }
        if !derivative_cutoff_hz.is_finite() || derivative_cutoff_hz <= 0.0 {
            return Err(FilterError::InvalidDerivativeCutoff);
        }

        Ok(Self {
            min_cutoff_hz,
            beta,
            derivative_cutoff_hz,
            previous_raw: None,
            previous_filtered: None,
            previous_derivative: 0.0,
        })
    }

    /// Filters one sample using elapsed time in seconds since the last sample.
    pub fn filter(&mut self, sample: f64, delta_seconds: f64) -> Result<f64, FilterError> {
        if !sample.is_finite() {
            return Err(FilterError::NonFiniteSample);
        }
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return Err(FilterError::InvalidDeltaTime);
        }

        let (Some(previous_raw), Some(previous_filtered)) =
            (self.previous_raw, self.previous_filtered)
        else {
            self.previous_raw = Some(sample);
            self.previous_filtered = Some(sample);
            return Ok(sample);
        };

        let raw_derivative = (sample - previous_raw) / delta_seconds;
        let derivative_alpha = smoothing_alpha(self.derivative_cutoff_hz, delta_seconds);
        let filtered_derivative =
            low_pass(raw_derivative, self.previous_derivative, derivative_alpha);

        let cutoff = self.min_cutoff_hz + self.beta * filtered_derivative.abs();
        let signal_alpha = smoothing_alpha(cutoff, delta_seconds);
        let filtered_sample = low_pass(sample, previous_filtered, signal_alpha);

        self.previous_raw = Some(sample);
        self.previous_filtered = Some(filtered_sample);
        self.previous_derivative = filtered_derivative;

        Ok(filtered_sample)
    }

    /// Clears history so the next sample initializes the filter.
    pub fn reset(&mut self) {
        self.previous_raw = None;
        self.previous_filtered = None;
        self.previous_derivative = 0.0;
    }
}

fn smoothing_alpha(cutoff_hz: f64, delta_seconds: f64) -> f64 {
    let tau = 1.0 / (TAU * cutoff_hz);
    1.0 / (1.0 + tau / delta_seconds)
}

fn low_pass(sample: f64, previous: f64, alpha: f64) -> f64 {
    alpha.mul_add(sample, (1.0 - alpha) * previous)
}

#[cfg(test)]
mod tests {
    use super::{FilterError, OneEuroFilter};

    #[test]
    fn first_sample_passes_through_unchanged() {
        let mut filter = OneEuroFilter::new(1.0, 0.0, 1.0).expect("valid filter");
        assert_eq!(filter.filter(42.0, 1.0 / 60.0), Ok(42.0));
    }

    #[test]
    fn reduces_stationary_high_frequency_jitter() {
        let mut filter = OneEuroFilter::new(1.0, 0.0, 1.0).expect("valid filter");
        let mut output = Vec::new();

        filter.filter(0.0, 0.01).expect("valid sample");
        for index in 0..100 {
            let sample = if index % 2 == 0 { 1.0 } else { -1.0 };
            output.push(filter.filter(sample, 0.01).expect("valid sample"));
        }

        let output_range = output.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - output.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(
            output_range < 0.2,
            "expected jitter to be strongly attenuated, got range {output_range}"
        );
    }

    #[test]
    fn higher_speed_increases_response() {
        let mut slow_filter = OneEuroFilter::new(1.0, 0.0, 1.0).expect("valid filter");
        let mut adaptive_filter = OneEuroFilter::new(1.0, 10.0, 1.0).expect("valid filter");

        slow_filter.filter(0.0, 0.01).expect("valid sample");
        adaptive_filter.filter(0.0, 0.01).expect("valid sample");

        let slow = slow_filter.filter(10.0, 0.01).expect("valid sample");
        let adaptive = adaptive_filter.filter(10.0, 0.01).expect("valid sample");

        assert!(adaptive > slow);
        assert!(adaptive <= 10.0);
    }

    #[test]
    fn reset_discards_previous_samples() {
        let mut filter = OneEuroFilter::new(1.0, 0.0, 1.0).expect("valid filter");
        filter.filter(0.0, 0.01).expect("valid sample");
        filter.filter(10.0, 0.01).expect("valid sample");
        filter.reset();

        assert_eq!(filter.filter(-5.0, 0.01), Ok(-5.0));
    }

    #[test]
    fn rejects_invalid_configuration_and_samples() {
        assert!(matches!(
            OneEuroFilter::new(0.0, 0.0, 1.0),
            Err(FilterError::InvalidMinCutoff)
        ));
        assert!(matches!(
            OneEuroFilter::new(1.0, -1.0, 1.0),
            Err(FilterError::InvalidBeta)
        ));
        assert!(matches!(
            OneEuroFilter::new(1.0, 0.0, f64::NAN),
            Err(FilterError::InvalidDerivativeCutoff)
        ));

        let mut filter = OneEuroFilter::new(1.0, 0.0, 1.0).expect("valid filter");
        assert_eq!(filter.filter(1.0, 0.0), Err(FilterError::InvalidDeltaTime));
        assert_eq!(
            filter.filter(f64::NAN, 0.01),
            Err(FilterError::NonFiniteSample)
        );
    }
}
