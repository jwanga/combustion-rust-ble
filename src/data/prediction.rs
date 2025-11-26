//! Prediction engine data structures.
//!
//! Contains types for managing the probe's temperature prediction system
//! which estimates when food will reach target temperatures.

/// The current state of the prediction engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum PredictionState {
    /// Probe is not inserted into food.
    #[default]
    ProbeNotInserted = 0,
    /// Probe is inserted but not yet predicting.
    ProbeInserted = 1,
    /// Probe is warming up, gathering initial data.
    Warming = 2,
    /// Actively predicting time to target.
    Predicting = 3,
    /// Prediction complete - remove from heat.
    RemovalPredictionDone = 4,
    /// Reserved for future use.
    ReservedState5 = 5,
    /// Reserved for future use.
    ReservedState6 = 6,
    /// Unknown state.
    Unknown = 7,
}

impl PredictionState {
    /// Create a PredictionState from a raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::ProbeNotInserted,
            1 => Self::ProbeInserted,
            2 => Self::Warming,
            3 => Self::Predicting,
            4 => Self::RemovalPredictionDone,
            5 => Self::ReservedState5,
            6 => Self::ReservedState6,
            _ => Self::Unknown,
        }
    }

    /// Convert to raw byte value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }

    /// Check if the prediction engine is actively predicting.
    pub fn is_predicting(&self) -> bool {
        matches!(self, Self::Predicting)
    }

    /// Check if the target temperature has been reached.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::RemovalPredictionDone)
    }
}

/// The mode of prediction being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum PredictionMode {
    /// No prediction active.
    #[default]
    None = 0,
    /// Predict time until removal from heat source.
    TimeToRemoval = 1,
    /// Predict time to removal and account for resting.
    RemovalAndResting = 2,
    /// Reserved for future use.
    Reserved = 3,
}

impl PredictionMode {
    /// Create a PredictionMode from a raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::TimeToRemoval,
            2 => Self::RemovalAndResting,
            _ => Self::Reserved,
        }
    }

    /// Convert to raw byte value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }
}

/// The type of prediction currently being calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum PredictionType {
    /// No prediction type.
    #[default]
    None = 0,
    /// Predicting when to remove from heat.
    Removal = 1,
    /// Predicting resting time after removal.
    Resting = 2,
    /// Reserved for future use.
    Reserved = 3,
}

impl PredictionType {
    /// Create a PredictionType from a raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Removal,
            2 => Self::Resting,
            _ => Self::Reserved,
        }
    }

    /// Convert to raw byte value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }
}

/// Complete prediction information from the probe.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PredictionInfo {
    /// Current state of the prediction engine.
    pub state: PredictionState,

    /// Mode of prediction being used.
    pub mode: PredictionMode,

    /// Type of prediction currently active.
    pub prediction_type: PredictionType,

    /// Target temperature in Celsius.
    pub set_point_temperature: f64,

    /// Temperature when the prediction started in Celsius.
    pub heat_start_temperature: f64,

    /// Predicted time remaining in seconds.
    pub prediction_value_seconds: u32,

    /// Current estimated core temperature in Celsius.
    pub estimated_core_temperature: f64,

    /// Time elapsed since prediction started in seconds.
    pub seconds_since_prediction_start: u32,

    /// Which sensor (0-7) is considered the core sensor.
    pub core_sensor_index: u8,
}

impl PredictionInfo {
    /// Create a new PredictionInfo with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the target temperature in Fahrenheit.
    pub fn set_point_fahrenheit(&self) -> f64 {
        crate::utils::celsius_to_fahrenheit(self.set_point_temperature)
    }

    /// Get the estimated core temperature in Fahrenheit.
    pub fn estimated_core_fahrenheit(&self) -> f64 {
        crate::utils::celsius_to_fahrenheit(self.estimated_core_temperature)
    }

    /// Get the heat start temperature in Fahrenheit.
    pub fn heat_start_fahrenheit(&self) -> f64 {
        crate::utils::celsius_to_fahrenheit(self.heat_start_temperature)
    }

    /// Get the predicted time remaining as minutes and seconds.
    pub fn prediction_time_formatted(&self) -> (u32, u32) {
        let minutes = self.prediction_value_seconds / 60;
        let seconds = self.prediction_value_seconds % 60;
        (minutes, seconds)
    }

    /// Check if the prediction is complete.
    pub fn is_complete(&self) -> bool {
        self.state.is_done()
    }

    /// Check if the prediction is actively running.
    pub fn is_active(&self) -> bool {
        self.state.is_predicting()
    }

    /// Calculate progress towards the target temperature as a percentage.
    ///
    /// Returns a value between 0.0 and 100.0, or None if the calculation
    /// is not possible (e.g., temperatures are equal).
    pub fn temperature_progress(&self) -> Option<f64> {
        let total_range = self.set_point_temperature - self.heat_start_temperature;
        if total_range.abs() < 0.001 {
            return None;
        }

        let current_progress = self.estimated_core_temperature - self.heat_start_temperature;
        let percentage = (current_progress / total_range) * 100.0;
        Some(percentage.clamp(0.0, 100.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prediction_state_from_raw() {
        assert_eq!(
            PredictionState::from_raw(0),
            PredictionState::ProbeNotInserted
        );
        assert_eq!(PredictionState::from_raw(3), PredictionState::Predicting);
        assert_eq!(
            PredictionState::from_raw(4),
            PredictionState::RemovalPredictionDone
        );
        assert_eq!(PredictionState::from_raw(255), PredictionState::Unknown);
    }

    #[test]
    fn test_prediction_state_methods() {
        assert!(PredictionState::Predicting.is_predicting());
        assert!(!PredictionState::Warming.is_predicting());
        assert!(PredictionState::RemovalPredictionDone.is_done());
        assert!(!PredictionState::Predicting.is_done());
    }

    #[test]
    fn test_prediction_mode_from_raw() {
        assert_eq!(PredictionMode::from_raw(0), PredictionMode::None);
        assert_eq!(PredictionMode::from_raw(1), PredictionMode::TimeToRemoval);
        assert_eq!(
            PredictionMode::from_raw(2),
            PredictionMode::RemovalAndResting
        );
        assert_eq!(PredictionMode::from_raw(100), PredictionMode::Reserved);
    }

    #[test]
    fn test_prediction_type_from_raw() {
        assert_eq!(PredictionType::from_raw(0), PredictionType::None);
        assert_eq!(PredictionType::from_raw(1), PredictionType::Removal);
        assert_eq!(PredictionType::from_raw(2), PredictionType::Resting);
    }

    #[test]
    fn test_prediction_info_time_formatted() {
        let info = PredictionInfo {
            prediction_value_seconds: 125,
            ..Default::default()
        };
        assert_eq!(info.prediction_time_formatted(), (2, 5));
    }

    #[test]
    fn test_prediction_info_temperature_progress() {
        let info = PredictionInfo {
            set_point_temperature: 63.0,
            heat_start_temperature: 20.0,
            estimated_core_temperature: 41.5,
            ..Default::default()
        };

        let progress = info.temperature_progress().unwrap();
        assert!((progress - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_prediction_info_temperature_progress_edge_cases() {
        // Same start and target temperature
        let info = PredictionInfo {
            set_point_temperature: 63.0,
            heat_start_temperature: 63.0,
            estimated_core_temperature: 63.0,
            ..Default::default()
        };
        assert!(info.temperature_progress().is_none());

        // Over target temperature
        let info = PredictionInfo {
            set_point_temperature: 63.0,
            heat_start_temperature: 20.0,
            estimated_core_temperature: 70.0,
            ..Default::default()
        };
        assert_eq!(info.temperature_progress().unwrap(), 100.0);
    }
}
