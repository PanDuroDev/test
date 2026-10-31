//! Exact frame-rate and timeline conversions.
//!
//! Time is stored as integer frame indices and a rational frame rate. Conversions
//! to nanoseconds are derived from the absolute frame index, never accumulated,
//! so fractional rates such as 24000/1001 do not accumulate floating-point drift.

/// A frame rate represented as a reduced rational number of frames per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

/// Errors produced while constructing a frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRateError {
    ZeroNumerator,
    ZeroDenominator,
}

impl FrameRate {
    /// Creates a positive frame rate and reduces it to its canonical fraction.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, FrameRateError> {
        if numerator == 0 {
            return Err(FrameRateError::ZeroNumerator);
        }
        if denominator == 0 {
            return Err(FrameRateError::ZeroDenominator);
        }

        let divisor = greatest_common_divisor(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    /// Returns the numerator of the reduced frames-per-second fraction.
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the denominator of the reduced frames-per-second fraction.
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Converts an absolute frame index to nanoseconds, rounding up to the first nanosecond at or after the frame boundary.
    ///
    /// The calculation uses integer arithmetic with a 128-bit intermediate.
    /// None indicates that the result cannot fit in a u64.
    pub fn frame_to_nanoseconds(self, frame_index: u64) -> Option<u64> {
        let scaled = u128::from(frame_index)
            .checked_mul(1_000_000_000)?
            .checked_mul(u128::from(self.denominator))?;
        let divisor = u128::from(self.numerator);
        let rounded_up = scaled.checked_add(divisor - 1)? / divisor;
        u64::try_from(rounded_up).ok()
    }

    /// Returns the frame that contains time_ns (floor semantics).
    pub fn nanoseconds_to_frame_floor(self, time_ns: u64) -> Option<u64> {
        let scaled = u128::from(time_ns).checked_mul(u128::from(self.numerator))?;
        let denominator = 1_000_000_000_u128.checked_mul(u128::from(self.denominator))?;
        u64::try_from(scaled / denominator).ok()
    }
}

fn greatest_common_divisor(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::{FrameRate, FrameRateError};

    #[test]
    fn rejects_zero_frame_rate_parts() {
        assert_eq!(FrameRate::new(0, 1), Err(FrameRateError::ZeroNumerator));
        assert_eq!(FrameRate::new(24, 0), Err(FrameRateError::ZeroDenominator));
    }

    #[test]
    fn reduces_equivalent_rates() {
        let rate = FrameRate::new(48_000, 2_000).expect("valid frame rate");
        assert_eq!(rate, FrameRate::new(24, 1).expect("valid frame rate"));
        assert_eq!(rate.numerator(), 24);
        assert_eq!(rate.denominator(), 1);
    }

    #[test]
    fn integer_rates_land_on_exact_second_boundaries() {
        for fps in [24, 30, 60] {
            let rate = FrameRate::new(fps, 1).expect("valid frame rate");
            assert_eq!(
                rate.frame_to_nanoseconds(u64::from(fps)),
                Some(1_000_000_000)
            );
            assert_eq!(
                rate.nanoseconds_to_frame_floor(1_000_000_000),
                Some(u64::from(fps))
            );
        }
    }

    #[test]
    fn fractional_rate_is_derived_from_absolute_frame_index() {
        let rate = FrameRate::new(24_000, 1_001).expect("valid frame rate");
        assert_eq!(rate.frame_to_nanoseconds(24_000), Some(1_001_000_000_000));
        assert_eq!(
            rate.nanoseconds_to_frame_floor(1_001_000_000_000),
            Some(24_000)
        );
    }

    #[test]
    fn frame_time_round_trip_uses_floor_semantics() {
        let rate = FrameRate::new(30_000, 1_001).expect("valid frame rate");
        for frame in [0, 1, 2, 99, 1_000, 100_000] {
            let time = rate.frame_to_nanoseconds(frame).expect("time fits");
            assert_eq!(rate.nanoseconds_to_frame_floor(time), Some(frame));
        }
    }
}
