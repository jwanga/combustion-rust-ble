//! Temperature log data structures.
//!
//! Contains types for storing and managing temperature history from probes.

use super::temperatures::ProbeTemperatures;
use chrono::{DateTime, Utc};

/// Prediction data logged with a temperature sample.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PredictionLog {
    /// Virtual core temperature at this sample.
    pub virtual_core: f64,

    /// Virtual surface temperature at this sample.
    pub virtual_surface: f64,

    /// Virtual ambient temperature at this sample.
    pub virtual_ambient: f64,

    /// Prediction state at this sample.
    pub prediction_state: u8,

    /// Set point temperature at this sample.
    pub prediction_set_point: f64,

    /// Prediction type at this sample.
    pub prediction_type: u8,

    /// Predicted seconds remaining at this sample.
    pub prediction_value_seconds: u32,
}

/// A single logged data point.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoggedDataPoint {
    /// Sequence number of this data point.
    pub sequence_number: u32,

    /// Temperature readings from all 8 sensors.
    pub temperatures: ProbeTemperatures,

    /// Optional prediction data associated with this sample.
    pub prediction_log: Option<PredictionLog>,

    /// Timestamp when this data point was logged (if known).
    pub timestamp: Option<DateTime<Utc>>,
}

impl LoggedDataPoint {
    /// Create a new LoggedDataPoint with just temperatures.
    pub fn new(sequence_number: u32, temperatures: ProbeTemperatures) -> Self {
        Self {
            sequence_number,
            temperatures,
            prediction_log: None,
            timestamp: None,
        }
    }

    /// Create a new LoggedDataPoint with prediction data.
    pub fn with_prediction(
        sequence_number: u32,
        temperatures: ProbeTemperatures,
        prediction: PredictionLog,
    ) -> Self {
        Self {
            sequence_number,
            temperatures,
            prediction_log: Some(prediction),
            timestamp: None,
        }
    }
}

/// Temperature log containing a session's data points.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TemperatureLog {
    /// Session ID this log belongs to.
    pub session_id: u32,

    /// Sample period in milliseconds.
    pub sample_period_ms: u32,

    /// All logged data points, sorted by sequence number.
    pub data_points: Vec<LoggedDataPoint>,
}

impl TemperatureLog {
    /// Create a new empty TemperatureLog.
    pub fn new(session_id: u32, sample_period_ms: u32) -> Self {
        Self {
            session_id,
            sample_period_ms,
            data_points: Vec::new(),
        }
    }

    /// Add a data point to the log.
    ///
    /// Points are inserted in sorted order by sequence number.
    pub fn add_data_point(&mut self, point: LoggedDataPoint) {
        // Find insertion point to maintain sorted order
        let pos = self
            .data_points
            .binary_search_by_key(&point.sequence_number, |p| p.sequence_number);

        match pos {
            Ok(_) => {
                // Duplicate sequence number - skip or replace
            }
            Err(insert_pos) => {
                self.data_points.insert(insert_pos, point);
            }
        }
    }

    /// Get the percentage of logs synced between min and max sequence.
    ///
    /// # Arguments
    ///
    /// * `min_seq` - Minimum sequence number on the probe
    /// * `max_seq` - Maximum sequence number on the probe
    ///
    /// # Returns
    ///
    /// Percentage (0.0 to 100.0) of data points received.
    pub fn percent_synced(&self, min_seq: u32, max_seq: u32) -> f64 {
        if max_seq <= min_seq {
            return 100.0;
        }

        let total_expected = (max_seq - min_seq + 1) as f64;
        let received = self.data_points.len() as f64;

        (received / total_expected * 100.0).min(100.0)
    }

    /// Get the number of data points in the log.
    pub fn len(&self) -> usize {
        self.data_points.len()
    }

    /// Check if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.data_points.is_empty()
    }

    /// Get the minimum sequence number in the log.
    pub fn min_sequence(&self) -> Option<u32> {
        self.data_points.first().map(|p| p.sequence_number)
    }

    /// Get the maximum sequence number in the log.
    pub fn max_sequence(&self) -> Option<u32> {
        self.data_points.last().map(|p| p.sequence_number)
    }

    /// Get missing sequence numbers in a range.
    pub fn missing_sequences(&self, min_seq: u32, max_seq: u32) -> Vec<u32> {
        let mut missing = Vec::new();
        let mut data_iter = self.data_points.iter().peekable();

        for seq in min_seq..=max_seq {
            // Skip data points with sequence less than current
            while data_iter
                .peek()
                .map(|p| p.sequence_number < seq)
                .unwrap_or(false)
            {
                data_iter.next();
            }

            // Check if current sequence exists
            if data_iter
                .peek()
                .map(|p| p.sequence_number != seq)
                .unwrap_or(true)
            {
                missing.push(seq);
            }
        }

        missing
    }

    /// Export the log to CSV format.
    ///
    /// # Returns
    ///
    /// A string containing CSV-formatted data with headers.
    pub fn to_csv(&self) -> String {
        let mut csv = String::new();

        // Header
        csv.push_str("Sequence,T1,T2,T3,T4,T5,T6,T7,T8");
        if self.data_points.iter().any(|p| p.prediction_log.is_some()) {
            csv.push_str(",VirtualCore,VirtualSurface,VirtualAmbient,PredictionState");
        }
        csv.push('\n');

        // Data rows
        for point in &self.data_points {
            csv.push_str(&format!("{}", point.sequence_number));

            for temp in &point.temperatures.values {
                if let Some(celsius) = temp.to_celsius() {
                    csv.push_str(&format!(",{:.2}", celsius));
                } else {
                    csv.push(',');
                }
            }

            if let Some(pred) = &point.prediction_log {
                csv.push_str(&format!(
                    ",{:.2},{:.2},{:.2},{}",
                    pred.virtual_core,
                    pred.virtual_surface,
                    pred.virtual_ambient,
                    pred.prediction_state
                ));
            }

            csv.push('\n');
        }

        csv
    }

    /// Calculate the duration of the log based on sequence numbers.
    pub fn duration(&self) -> std::time::Duration {
        if self.data_points.is_empty() || self.sample_period_ms == 0 {
            return std::time::Duration::ZERO;
        }

        let min_seq = self.min_sequence().unwrap_or(0);
        let max_seq = self.max_sequence().unwrap_or(0);
        let samples = max_seq.saturating_sub(min_seq);

        std::time::Duration::from_millis(samples as u64 * self.sample_period_ms as u64)
    }
}

impl Default for TemperatureLog {
    fn default() -> Self {
        Self::new(0, 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::temperatures::RawTemperature;

    fn make_temperatures(base: u16) -> ProbeTemperatures {
        ProbeTemperatures {
            values: [
                RawTemperature::new(base),
                RawTemperature::new(base + 10),
                RawTemperature::new(base + 20),
                RawTemperature::new(base + 30),
                RawTemperature::new(base + 40),
                RawTemperature::new(base + 50),
                RawTemperature::new(base + 60),
                RawTemperature::new(base + 70),
            ],
        }
    }

    #[test]
    fn test_temperature_log_new() {
        let log = TemperatureLog::new(0x12345678, 1000);
        assert_eq!(log.session_id, 0x12345678);
        assert_eq!(log.sample_period_ms, 1000);
        assert!(log.is_empty());
    }

    #[test]
    fn test_temperature_log_add_data_point() {
        let mut log = TemperatureLog::new(0, 1000);

        log.add_data_point(LoggedDataPoint::new(10, make_temperatures(1000)));
        log.add_data_point(LoggedDataPoint::new(5, make_temperatures(1100)));
        log.add_data_point(LoggedDataPoint::new(15, make_temperatures(1200)));

        assert_eq!(log.len(), 3);
        assert_eq!(log.data_points[0].sequence_number, 5);
        assert_eq!(log.data_points[1].sequence_number, 10);
        assert_eq!(log.data_points[2].sequence_number, 15);
    }

    #[test]
    fn test_temperature_log_percent_synced() {
        let mut log = TemperatureLog::new(0, 1000);

        for i in 0..50 {
            log.add_data_point(LoggedDataPoint::new(i, make_temperatures(1000)));
        }

        assert!((log.percent_synced(0, 99) - 50.0).abs() < 0.1);
        assert!((log.percent_synced(0, 49) - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_temperature_log_missing_sequences() {
        let mut log = TemperatureLog::new(0, 1000);

        log.add_data_point(LoggedDataPoint::new(0, make_temperatures(1000)));
        log.add_data_point(LoggedDataPoint::new(2, make_temperatures(1000)));
        log.add_data_point(LoggedDataPoint::new(5, make_temperatures(1000)));

        let missing = log.missing_sequences(0, 5);
        assert_eq!(missing, vec![1, 3, 4]);
    }

    #[test]
    fn test_temperature_log_to_csv() {
        let mut log = TemperatureLog::new(0, 1000);
        log.add_data_point(LoggedDataPoint::new(0, make_temperatures(1000)));

        let csv = log.to_csv();
        assert!(csv.contains("Sequence,T1,T2,T3,T4,T5,T6,T7,T8"));
        assert!(csv.contains("0,"));
    }

    #[test]
    fn test_temperature_log_duration() {
        let mut log = TemperatureLog::new(0, 1000);

        log.add_data_point(LoggedDataPoint::new(0, make_temperatures(1000)));
        log.add_data_point(LoggedDataPoint::new(60, make_temperatures(1000)));

        let duration = log.duration();
        assert_eq!(duration, std::time::Duration::from_secs(60));
    }
}
