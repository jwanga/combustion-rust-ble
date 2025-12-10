//! Temperature alarm data structures.
//!
//! Contains types for managing high and low temperature alarms on the probe.
//! Based on the Combustion Probe BLE Specification.

/// Alarm status for a single temperature alarm.
///
/// Each alarm is a 16-bit packed structure:
/// - Bit 0: Set (alarm is enabled)
/// - Bit 1: Tripped (alarm has been triggered)
/// - Bit 2: Alarming (alarm is currently sounding)
/// - Bits 3-15: Temperature (13 bits, formula: `(raw × 0.1) - 20`)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AlarmStatus {
    /// Whether the alarm is enabled.
    pub set: bool,
    /// Whether the alarm has been triggered.
    pub tripped: bool,
    /// Whether the alarm is currently sounding.
    pub alarming: bool,
    /// The alarm threshold temperature in Celsius.
    /// Range: -20°C to 799°C with 0.1°C resolution.
    pub temperature: f64,
}

impl AlarmStatus {
    /// Size of a single alarm status in bytes.
    pub const SIZE: usize = 2;

    /// Create a new alarm status.
    pub fn new(temperature: f64, enabled: bool) -> Self {
        Self {
            set: enabled,
            tripped: false,
            alarming: false,
            temperature,
        }
    }

    /// Create a disabled alarm.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Parse from a 2-byte packed structure.
    ///
    /// Format (16 bits):
    /// - Bit 0: Set
    /// - Bit 1: Tripped
    /// - Bit 2: Alarming
    /// - Bits 3-15: Temperature (13 bits)
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }

        let packed = u16::from_le_bytes([bytes[0], bytes[1]]);

        let set = (packed & 0x01) != 0;
        let tripped = (packed & 0x02) != 0;
        let alarming = (packed & 0x04) != 0;

        // Temperature is in bits 3-15 (13 bits)
        let temp_raw = (packed >> 3) & 0x1FFF;
        let temperature = (temp_raw as f64 * 0.1) - 20.0;

        Some(Self {
            set,
            tripped,
            alarming,
            temperature,
        })
    }

    /// Encode to a 2-byte packed structure.
    pub fn to_bytes(&self) -> [u8; 2] {
        // Encode temperature: (celsius + 20) / 0.1, clamped to 13 bits
        let temp_raw = ((self.temperature + 20.0) / 0.1).round() as u16;
        let temp_raw = temp_raw.min(0x1FFF); // 13-bit max

        let mut packed: u16 = 0;
        if self.set {
            packed |= 0x01;
        }
        if self.tripped {
            packed |= 0x02;
        }
        if self.alarming {
            packed |= 0x04;
        }
        packed |= (temp_raw & 0x1FFF) << 3;

        packed.to_le_bytes()
    }

    /// Check if the alarm is enabled.
    pub fn is_enabled(&self) -> bool {
        self.set
    }

    /// Check if the alarm has been triggered.
    pub fn is_tripped(&self) -> bool {
        self.tripped
    }

    /// Check if the alarm is currently sounding.
    pub fn is_alarming(&self) -> bool {
        self.alarming
    }

    /// Get the temperature in Fahrenheit.
    pub fn temperature_fahrenheit(&self) -> f64 {
        crate::utils::celsius_to_fahrenheit(self.temperature)
    }
}

/// Number of alarms in each array (one per sensor + 3 virtual sensors).
pub const ALARM_COUNT: usize = 11;

/// Size of the alarm array in bytes (11 alarms × 2 bytes each).
pub const ALARM_ARRAY_SIZE: usize = ALARM_COUNT * AlarmStatus::SIZE;

/// High and low temperature alarm configuration for all sensors.
///
/// The probe supports alarms for:
/// - 8 physical sensors (T1-T8)
/// - 3 virtual sensors (Core, Surface, Ambient)
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AlarmConfig {
    /// High temperature alarms (11 entries: T1-T8 + Core, Surface, Ambient).
    pub high_alarms: [AlarmStatus; ALARM_COUNT],
    /// Low temperature alarms (11 entries: T1-T8 + Core, Surface, Ambient).
    pub low_alarms: [AlarmStatus; ALARM_COUNT],
}

impl AlarmConfig {
    /// Total size of the alarm configuration in bytes (44 bytes).
    pub const SIZE: usize = ALARM_ARRAY_SIZE * 2;

    /// Create a new alarm configuration with all alarms disabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a high temperature alarm for a specific sensor.
    ///
    /// # Arguments
    /// * `sensor_index` - Sensor index (0-7 for T1-T8, 8=Core, 9=Surface, 10=Ambient)
    /// * `temperature` - Alarm threshold in Celsius
    /// * `enabled` - Whether the alarm is enabled
    pub fn set_high_alarm(&mut self, sensor_index: usize, temperature: f64, enabled: bool) {
        if sensor_index < ALARM_COUNT {
            self.high_alarms[sensor_index] = AlarmStatus::new(temperature, enabled);
        }
    }

    /// Set a low temperature alarm for a specific sensor.
    ///
    /// # Arguments
    /// * `sensor_index` - Sensor index (0-7 for T1-T8, 8=Core, 9=Surface, 10=Ambient)
    /// * `temperature` - Alarm threshold in Celsius
    /// * `enabled` - Whether the alarm is enabled
    pub fn set_low_alarm(&mut self, sensor_index: usize, temperature: f64, enabled: bool) {
        if sensor_index < ALARM_COUNT {
            self.low_alarms[sensor_index] = AlarmStatus::new(temperature, enabled);
        }
    }

    /// Set high alarm for the core (virtual) sensor.
    pub fn set_core_high_alarm(&mut self, temperature: f64, enabled: bool) {
        self.set_high_alarm(8, temperature, enabled);
    }

    /// Set low alarm for the core (virtual) sensor.
    pub fn set_core_low_alarm(&mut self, temperature: f64, enabled: bool) {
        self.set_low_alarm(8, temperature, enabled);
    }

    /// Set high alarm for the surface (virtual) sensor.
    pub fn set_surface_high_alarm(&mut self, temperature: f64, enabled: bool) {
        self.set_high_alarm(9, temperature, enabled);
    }

    /// Set low alarm for the surface (virtual) sensor.
    pub fn set_surface_low_alarm(&mut self, temperature: f64, enabled: bool) {
        self.set_low_alarm(9, temperature, enabled);
    }

    /// Set high alarm for the ambient (virtual) sensor.
    pub fn set_ambient_high_alarm(&mut self, temperature: f64, enabled: bool) {
        self.set_high_alarm(10, temperature, enabled);
    }

    /// Set low alarm for the ambient (virtual) sensor.
    pub fn set_ambient_low_alarm(&mut self, temperature: f64, enabled: bool) {
        self.set_low_alarm(10, temperature, enabled);
    }

    /// Get high alarm for a specific sensor.
    pub fn high_alarm(&self, sensor_index: usize) -> Option<&AlarmStatus> {
        self.high_alarms.get(sensor_index)
    }

    /// Get low alarm for a specific sensor.
    pub fn low_alarm(&self, sensor_index: usize) -> Option<&AlarmStatus> {
        self.low_alarms.get(sensor_index)
    }

    /// Get the core high alarm.
    pub fn core_high_alarm(&self) -> &AlarmStatus {
        &self.high_alarms[8]
    }

    /// Get the core low alarm.
    pub fn core_low_alarm(&self) -> &AlarmStatus {
        &self.low_alarms[8]
    }

    /// Parse from bytes (44 bytes: 22 high + 22 low).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }

        let mut high_alarms = [AlarmStatus::default(); ALARM_COUNT];
        let mut low_alarms = [AlarmStatus::default(); ALARM_COUNT];

        // Parse high alarms (first 22 bytes)
        for i in 0..ALARM_COUNT {
            let offset = i * 2;
            high_alarms[i] = AlarmStatus::from_bytes(&bytes[offset..offset + 2])?;
        }

        // Parse low alarms (next 22 bytes)
        for i in 0..ALARM_COUNT {
            let offset = ALARM_ARRAY_SIZE + i * 2;
            low_alarms[i] = AlarmStatus::from_bytes(&bytes[offset..offset + 2])?;
        }

        Some(Self {
            high_alarms,
            low_alarms,
        })
    }

    /// Encode to bytes (44 bytes: 22 high + 22 low).
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];

        // Encode high alarms (first 22 bytes)
        for (i, alarm) in self.high_alarms.iter().enumerate() {
            let offset = i * 2;
            let alarm_bytes = alarm.to_bytes();
            bytes[offset] = alarm_bytes[0];
            bytes[offset + 1] = alarm_bytes[1];
        }

        // Encode low alarms (next 22 bytes)
        for (i, alarm) in self.low_alarms.iter().enumerate() {
            let offset = ALARM_ARRAY_SIZE + i * 2;
            let alarm_bytes = alarm.to_bytes();
            bytes[offset] = alarm_bytes[0];
            bytes[offset + 1] = alarm_bytes[1];
        }

        bytes
    }

    /// Check if any alarm is currently triggered.
    pub fn any_tripped(&self) -> bool {
        self.high_alarms.iter().any(|a| a.tripped) || self.low_alarms.iter().any(|a| a.tripped)
    }

    /// Check if any alarm is currently sounding.
    pub fn any_alarming(&self) -> bool {
        self.high_alarms.iter().any(|a| a.alarming) || self.low_alarms.iter().any(|a| a.alarming)
    }

    /// Check if any alarm is enabled.
    pub fn any_enabled(&self) -> bool {
        self.high_alarms.iter().any(|a| a.set) || self.low_alarms.iter().any(|a| a.set)
    }

    /// Get all triggered high alarms with their sensor indices.
    pub fn triggered_high_alarms(&self) -> Vec<(usize, &AlarmStatus)> {
        self.high_alarms
            .iter()
            .enumerate()
            .filter(|(_, a)| a.tripped)
            .collect()
    }

    /// Get all triggered low alarms with their sensor indices.
    pub fn triggered_low_alarms(&self) -> Vec<(usize, &AlarmStatus)> {
        self.low_alarms
            .iter()
            .enumerate()
            .filter(|(_, a)| a.tripped)
            .collect()
    }

    /// Get the sensor name for an alarm index.
    pub fn sensor_name(index: usize) -> &'static str {
        match index {
            0 => "T1",
            1 => "T2",
            2 => "T3",
            3 => "T4",
            4 => "T5",
            5 => "T6",
            6 => "T7",
            7 => "T8",
            8 => "Core",
            9 => "Surface",
            10 => "Ambient",
            _ => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alarm_status_roundtrip() {
        let alarm = AlarmStatus {
            set: true,
            tripped: false,
            alarming: true,
            temperature: 63.0,
        };

        let bytes = alarm.to_bytes();
        let parsed = AlarmStatus::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.set, alarm.set);
        assert_eq!(parsed.tripped, alarm.tripped);
        assert_eq!(parsed.alarming, alarm.alarming);
        assert!((parsed.temperature - alarm.temperature).abs() < 0.1);
    }

    #[test]
    fn test_alarm_status_temperature_encoding() {
        // Test various temperatures
        let temps = [-20.0, 0.0, 25.0, 63.0, 100.0, 200.0, 500.0];

        for temp in temps {
            let alarm = AlarmStatus::new(temp, true);
            let bytes = alarm.to_bytes();
            let parsed = AlarmStatus::from_bytes(&bytes).unwrap();
            assert!(
                (parsed.temperature - temp).abs() < 0.1,
                "Temperature mismatch: expected {}, got {}",
                temp,
                parsed.temperature
            );
        }
    }

    #[test]
    fn test_alarm_config_roundtrip() {
        let mut config = AlarmConfig::new();
        config.set_core_high_alarm(74.0, true);
        config.set_core_low_alarm(4.0, true);
        config.set_high_alarm(0, 200.0, true); // T1 high

        let bytes = config.to_bytes();
        let parsed = AlarmConfig::from_bytes(&bytes).unwrap();

        assert!(parsed.core_high_alarm().is_enabled());
        assert!((parsed.core_high_alarm().temperature - 74.0).abs() < 0.1);
        assert!(parsed.core_low_alarm().is_enabled());
        assert!((parsed.core_low_alarm().temperature - 4.0).abs() < 0.1);
        assert!(parsed.high_alarm(0).unwrap().is_enabled());
    }

    #[test]
    fn test_alarm_config_any_methods() {
        let mut config = AlarmConfig::new();
        assert!(!config.any_enabled());
        assert!(!config.any_tripped());
        assert!(!config.any_alarming());

        config.set_core_high_alarm(74.0, true);
        assert!(config.any_enabled());

        config.high_alarms[8].tripped = true;
        assert!(config.any_tripped());

        config.high_alarms[8].alarming = true;
        assert!(config.any_alarming());
    }

    #[test]
    fn test_sensor_names() {
        assert_eq!(AlarmConfig::sensor_name(0), "T1");
        assert_eq!(AlarmConfig::sensor_name(7), "T8");
        assert_eq!(AlarmConfig::sensor_name(8), "Core");
        assert_eq!(AlarmConfig::sensor_name(9), "Surface");
        assert_eq!(AlarmConfig::sensor_name(10), "Ambient");
    }
}
