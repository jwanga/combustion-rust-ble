//! Session information data structures.
//!
//! Contains types for managing probe cooking sessions.

/// Information about a cooking session.
///
/// A session begins when the probe leaves the charger and continues
/// until it returns. Each session has a unique random ID.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SessionInfo {
    /// Random ID generated when probe leaves charger.
    pub session_id: u32,

    /// Milliseconds between log samples.
    ///
    /// Typical values are 1000ms (1 second) during normal operation.
    pub sample_period_ms: u32,
}

impl SessionInfo {
    /// Create a new SessionInfo with the specified values.
    pub fn new(session_id: u32, sample_period_ms: u32) -> Self {
        Self {
            session_id,
            sample_period_ms,
        }
    }

    /// Get the sample period as a duration.
    pub fn sample_period(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.sample_period_ms as u64)
    }

    /// Get the sample rate in Hz.
    pub fn sample_rate_hz(&self) -> f64 {
        if self.sample_period_ms == 0 {
            0.0
        } else {
            1000.0 / self.sample_period_ms as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_session_info_new() {
        let session = SessionInfo::new(0x12345678, 1000);
        assert_eq!(session.session_id, 0x12345678);
        assert_eq!(session.sample_period_ms, 1000);
    }

    #[test]
    fn test_session_info_sample_period() {
        let session = SessionInfo::new(0, 1000);
        assert_eq!(session.sample_period(), Duration::from_millis(1000));
    }

    #[test]
    fn test_session_info_sample_rate() {
        let session = SessionInfo::new(0, 1000);
        assert!((session.sample_rate_hz() - 1.0).abs() < 0.001);

        let session = SessionInfo::new(0, 500);
        assert!((session.sample_rate_hz() - 2.0).abs() < 0.001);

        let session = SessionInfo::new(0, 0);
        assert_eq!(session.sample_rate_hz(), 0.0);
    }
}
