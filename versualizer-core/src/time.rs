//! Time and duration conversion utilities.
//!
//! This module provides safe conversion functions for durations,
//! avoiding truncation issues with explicit saturation behavior.

use std::time::Duration;

/// Extension trait for safe Duration conversions.
pub trait DurationExt {
    /// Convert duration to milliseconds as u64, saturating at `u64::MAX`.
    ///
    /// In practice, this is always safe because durations exceeding `u64::MAX`
    /// milliseconds would represent ~584 million years.
    fn as_millis_u64(&self) -> u64;

    /// Convert duration to milliseconds as i64, saturating at `i64::MAX`.
    ///
    /// Useful for database storage. In practice, this is always safe because
    /// durations exceeding `i64::MAX` milliseconds would represent ~292 million years.
    fn as_millis_i64(&self) -> i64;

    /// Convert duration to seconds as u32, saturating at `u32::MAX`.
    ///
    /// In practice, this is always safe for audio tracks because
    /// `u32::MAX` seconds is approximately 136 years.
    fn as_secs_u32(&self) -> u32;
}

impl DurationExt for Duration {
    fn as_millis_u64(&self) -> u64 {
        u64::try_from(self.as_millis()).unwrap_or(u64::MAX)
    }

    fn as_millis_i64(&self) -> i64 {
        i64::try_from(self.as_millis()).unwrap_or(i64::MAX)
    }

    fn as_secs_u32(&self) -> u32 {
        u32::try_from(self.as_secs()).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_millis_u64() {
        let duration = Duration::from_millis(1234);
        assert_eq!(duration.as_millis_u64(), 1234);
    }

    #[test]
    fn test_as_millis_u64_zero() {
        let duration = Duration::ZERO;
        assert_eq!(duration.as_millis_u64(), 0);
    }

    #[test]
    fn test_as_millis_i64() {
        let duration = Duration::from_millis(5000);
        assert_eq!(duration.as_millis_i64(), 5000);
    }

    #[test]
    fn test_as_millis_i64_zero() {
        let duration = Duration::ZERO;
        assert_eq!(duration.as_millis_i64(), 0);
    }

    #[test]
    fn test_as_secs_u32() {
        let duration = Duration::from_secs(300);
        assert_eq!(duration.as_secs_u32(), 300);
    }

    #[test]
    fn test_as_secs_u32_large() {
        // Duration larger than u32::MAX seconds
        let duration = Duration::from_secs(u64::from(u32::MAX) + 1);
        assert_eq!(duration.as_secs_u32(), u32::MAX);
    }

    #[test]
    fn test_as_secs_u32_zero() {
        let duration = Duration::ZERO;
        assert_eq!(duration.as_secs_u32(), 0);
    }
}
