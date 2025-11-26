//! Temperature data structures.
//!
//! Contains types for raw temperature values from sensors and
//! virtual temperature calculations.

use crate::utils::{celsius_to_fahrenheit, fahrenheit_to_celsius};

/// Raw temperature value from a sensor (13-bit).
///
/// The probe reports 13-bit temperature values that need to be converted
/// to actual temperature readings. The conversion formula is:
/// `temperature_celsius = (raw_value * 0.05) - 20.0`
///
/// This provides a range of -20°C to ~389°C with 0.05°C resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawTemperature(pub u16);

impl RawTemperature {
    /// The maximum valid raw temperature value (13-bit).
    pub const MAX_VALUE: u16 = 0x1FFE;

    /// Invalid temperature marker (all 1s in 13-bit value).
    pub const INVALID: Self = Self(0x1FFF);

    /// Create a new RawTemperature from a raw 13-bit value.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw 13-bit temperature value (0-8191)
    pub fn new(value: u16) -> Self {
        Self(value & 0x1FFF)
    }

    /// Check if this temperature value is valid.
    ///
    /// A value of 0x1FFF (8191) indicates an invalid reading.
    pub fn is_valid(&self) -> bool {
        self.0 != 0x1FFF
    }

    /// Convert the raw value to Celsius.
    ///
    /// # Returns
    ///
    /// `Some(temperature)` if the value is valid, `None` if invalid.
    ///
    /// # Example
    ///
    /// ```
    /// use combustion_rust_ble::data::RawTemperature;
    ///
    /// // 0°C = raw value 400 (because 400 * 0.05 - 20 = 0)
    /// let temp = RawTemperature::new(400);
    /// assert_eq!(temp.to_celsius(), Some(0.0));
    ///
    /// // 100°C = raw value 2400 (because 2400 * 0.05 - 20 = 100)
    /// let temp = RawTemperature::new(2400);
    /// assert_eq!(temp.to_celsius(), Some(100.0));
    /// ```
    pub fn to_celsius(&self) -> Option<f64> {
        if self.0 == 0x1FFF {
            None
        } else {
            Some((self.0 as f64 * 0.05) - 20.0)
        }
    }

    /// Convert the raw value to Fahrenheit.
    ///
    /// # Returns
    ///
    /// `Some(temperature)` if the value is valid, `None` if invalid.
    pub fn to_fahrenheit(&self) -> Option<f64> {
        self.to_celsius().map(celsius_to_fahrenheit)
    }

    /// Create a RawTemperature from a Celsius value.
    ///
    /// # Arguments
    ///
    /// * `celsius` - Temperature in degrees Celsius
    ///
    /// # Returns
    ///
    /// The corresponding RawTemperature value
    pub fn from_celsius(celsius: f64) -> Self {
        // Inverse of: celsius = raw * 0.05 - 20
        // raw = (celsius + 20) / 0.05 = (celsius + 20) * 20
        let raw = ((celsius + 20.0) * 20.0).round() as u16;
        Self(raw.min(Self::MAX_VALUE))
    }

    /// Create a RawTemperature from a Fahrenheit value.
    ///
    /// # Arguments
    ///
    /// * `fahrenheit` - Temperature in degrees Fahrenheit
    ///
    /// # Returns
    ///
    /// The corresponding RawTemperature value
    pub fn from_fahrenheit(fahrenheit: f64) -> Self {
        Self::from_celsius(fahrenheit_to_celsius(fahrenheit))
    }

    /// Get the raw 13-bit value.
    pub fn raw_value(&self) -> u16 {
        self.0
    }
}

impl Default for RawTemperature {
    fn default() -> Self {
        Self::INVALID
    }
}

/// All 8 temperature readings from a probe.
///
/// The probe has 8 temperature sensors arranged from tip to handle:
/// - T1: High-precision sensor at tip (core temperature candidate)
/// - T2: High-precision sensor
/// - T3: MCU temperature sensor
/// - T4: High-precision sensor
/// - T5: High-temperature thermistor
/// - T6: High-temperature thermistor
/// - T7: High-temperature thermistor
/// - T8: High-temperature thermistor at handle (ambient temperature)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProbeTemperatures {
    /// Raw temperature values for all 8 sensors (T1-T8).
    pub values: [RawTemperature; 8],
}

impl ProbeTemperatures {
    /// Create a new ProbeTemperatures with all invalid values.
    pub fn new() -> Self {
        Self {
            values: [RawTemperature::INVALID; 8],
        }
    }

    /// Create ProbeTemperatures from raw values.
    ///
    /// # Arguments
    ///
    /// * `values` - Array of 8 raw temperature values
    pub fn from_raw(values: [u16; 8]) -> Self {
        Self {
            values: values.map(RawTemperature::new),
        }
    }

    /// Get temperature for a specific sensor index.
    ///
    /// # Arguments
    ///
    /// * `index` - Sensor index (0-7, where 0 is T1 at tip)
    ///
    /// # Returns
    ///
    /// Reference to the RawTemperature, or None if index is out of bounds.
    pub fn sensor(&self, index: usize) -> Option<&RawTemperature> {
        self.values.get(index)
    }

    /// Get all temperatures in Celsius.
    ///
    /// # Returns
    ///
    /// Array of 8 `Option<f64>` values, where None indicates an invalid reading.
    pub fn to_celsius(&self) -> [Option<f64>; 8] {
        self.values.map(|t| t.to_celsius())
    }

    /// Get all temperatures in Fahrenheit.
    ///
    /// # Returns
    ///
    /// Array of 8 `Option<f64>` values, where None indicates an invalid reading.
    pub fn to_fahrenheit(&self) -> [Option<f64>; 8] {
        self.values.map(|t| t.to_fahrenheit())
    }

    /// Parse temperatures from packed 13-byte advertising data.
    ///
    /// The 8 temperatures are packed as 13-bit values in 13 bytes (104 bits).
    ///
    /// # Arguments
    ///
    /// * `data` - 13 bytes of packed temperature data
    ///
    /// # Returns
    ///
    /// Parsed ProbeTemperatures, or None if data is wrong length.
    pub fn from_packed_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 13 {
            return None;
        }

        let mut values = [RawTemperature::INVALID; 8];

        // Unpack 8 13-bit values from 13 bytes
        // Each temperature is 13 bits, packed sequentially
        let mut bit_offset = 0;

        for (i, temp) in values.iter_mut().enumerate() {
            let byte_offset = bit_offset / 8;
            let bit_position = bit_offset % 8;

            // Extract 13-bit value spanning potentially 3 bytes
            let raw_value: u16 = if bit_position <= 3 {
                // Value fits in 2 bytes
                ((data[byte_offset] as u16) >> bit_position)
                    | ((data[byte_offset + 1] as u16) << (8 - bit_position))
            } else {
                // Value spans 3 bytes
                ((data[byte_offset] as u16) >> bit_position)
                    | ((data[byte_offset + 1] as u16) << (8 - bit_position))
                    | ((data[byte_offset + 2] as u16) << (16 - bit_position))
            } & 0x1FFF;
            *temp = RawTemperature::new(raw_value);

            bit_offset += 13;
            let _ = i; // Suppress unused variable warning
        }

        Some(Self { values })
    }

    /// Pack temperatures into 13-byte format for transmission.
    ///
    /// # Returns
    ///
    /// 13 bytes containing packed temperature data.
    pub fn to_packed_bytes(&self) -> [u8; 13] {
        let mut result = [0u8; 13];
        let mut bit_offset = 0;

        for temp in &self.values {
            let byte_offset = bit_offset / 8;
            let bit_position = bit_offset % 8;
            let raw_value = temp.0 & 0x1FFF;

            // Write 13-bit value
            result[byte_offset] |= (raw_value << bit_position) as u8;
            result[byte_offset + 1] |= (raw_value >> (8 - bit_position)) as u8;
            if bit_position > 3 && byte_offset + 2 < 13 {
                result[byte_offset + 2] |= (raw_value >> (16 - bit_position)) as u8;
            }

            bit_offset += 13;
        }

        result
    }
}

impl Default for ProbeTemperatures {
    fn default() -> Self {
        Self::new()
    }
}

/// Virtual sensor selection - which physical sensors are being used for virtual temperatures.
///
/// The probe dynamically selects which physical sensors (T1-T8) to use for
/// core, surface, and ambient readings based on insertion depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualSensorSelection {
    /// Physical sensor index (0-5) used for core temperature (T1-T6).
    pub core_sensor: u8,
    /// Physical sensor index (3-6) used for surface temperature (T4-T7).
    pub surface_sensor: u8,
    /// Physical sensor index (4-7) used for ambient temperature (T5-T8).
    pub ambient_sensor: u8,
}

impl VirtualSensorSelection {
    /// Create a new virtual sensor selection from raw sensor indices.
    pub fn new(core_sensor: u8, surface_sensor: u8, ambient_sensor: u8) -> Self {
        Self {
            core_sensor,
            surface_sensor,
            ambient_sensor,
        }
    }

    /// Parse virtual sensor selection from the selection byte.
    ///
    /// The byte encodes which physical sensor to use:
    /// - Bits 0-2: Virtual Core sensor (selects from T1-T6, values 0-5)
    /// - Bits 3-4: Virtual Surface sensor (selects from T4-T7, values 0-3 map to T4-T7)
    /// - Bits 5-6: Virtual Ambient sensor (selects from T5-T8, values 0-3 map to T5-T8)
    pub fn from_byte(byte: u8) -> Self {
        let core_sensor = byte & 0x07;
        let surface_sensor = ((byte >> 3) & 0x03) + 3;
        let ambient_sensor = ((byte >> 5) & 0x03) + 4;
        Self {
            core_sensor,
            surface_sensor,
            ambient_sensor,
        }
    }

    /// Get the display name for the core sensor (e.g., "T1", "T2", etc.).
    pub fn core_sensor_name(&self) -> String {
        format!("T{}", self.core_sensor + 1)
    }

    /// Get the display name for the surface sensor (e.g., "T4", "T5", etc.).
    pub fn surface_sensor_name(&self) -> String {
        format!("T{}", self.surface_sensor + 1)
    }

    /// Get the display name for the ambient sensor (e.g., "T5", "T6", etc.).
    pub fn ambient_sensor_name(&self) -> String {
        format!("T{}", self.ambient_sensor + 1)
    }
}

/// Virtual temperatures calculated from raw sensor data.
///
/// These temperatures are computed by the probe's firmware based on the
/// raw sensor readings and the probe's insertion depth into the food.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualTemperatures {
    /// Core temperature (lowest internal temperature).
    ///
    /// This represents the coldest part of the food being monitored.
    pub core: Option<f64>,

    /// Surface temperature (temperature at food surface).
    ///
    /// This represents the temperature where the food meets the cooking medium.
    pub surface: Option<f64>,

    /// Ambient temperature (air temperature near food).
    ///
    /// This represents the temperature of the cooking environment.
    pub ambient: Option<f64>,

    /// Which physical sensors are being used for these virtual readings.
    pub sensor_selection: VirtualSensorSelection,
}

impl VirtualTemperatures {
    /// Create new virtual temperatures with all values set.
    pub fn new(core: Option<f64>, surface: Option<f64>, ambient: Option<f64>) -> Self {
        Self {
            core,
            surface,
            ambient,
            sensor_selection: VirtualSensorSelection::default(),
        }
    }

    /// Create new virtual temperatures with sensor selection info.
    pub fn with_selection(
        core: Option<f64>,
        surface: Option<f64>,
        ambient: Option<f64>,
        sensor_selection: VirtualSensorSelection,
    ) -> Self {
        Self {
            core,
            surface,
            ambient,
            sensor_selection,
        }
    }

    /// Get core temperature in Fahrenheit.
    pub fn core_fahrenheit(&self) -> Option<f64> {
        self.core.map(celsius_to_fahrenheit)
    }

    /// Get surface temperature in Fahrenheit.
    pub fn surface_fahrenheit(&self) -> Option<f64> {
        self.surface.map(celsius_to_fahrenheit)
    }

    /// Get ambient temperature in Fahrenheit.
    pub fn ambient_fahrenheit(&self) -> Option<f64> {
        self.ambient.map(celsius_to_fahrenheit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_temperature_to_celsius() {
        // Formula: celsius = raw * 0.05 - 20
        // 0°C = raw value 400 (400 * 0.05 - 20 = 0)
        assert_eq!(RawTemperature(400).to_celsius(), Some(0.0));

        // 100°C = raw value 2400 (2400 * 0.05 - 20 = 100)
        assert_eq!(RawTemperature(2400).to_celsius(), Some(100.0));

        // -20°C = raw value 0 (0 * 0.05 - 20 = -20)
        assert_eq!(RawTemperature(0).to_celsius(), Some(-20.0));

        // Room temp ~23°C = raw value 860 (860 * 0.05 - 20 = 23)
        assert_eq!(RawTemperature(860).to_celsius(), Some(23.0));

        // Invalid value
        assert_eq!(RawTemperature(0x1FFF).to_celsius(), None);
    }

    #[test]
    fn test_raw_temperature_to_fahrenheit() {
        // 0°C = 32°F, raw = 400
        let temp = RawTemperature(400);
        assert!((temp.to_fahrenheit().unwrap() - 32.0).abs() < 0.001);

        // 100°C = 212°F, raw = 2400
        let temp = RawTemperature(2400);
        assert!((temp.to_fahrenheit().unwrap() - 212.0).abs() < 0.001);
    }

    #[test]
    fn test_raw_temperature_from_celsius() {
        // Inverse: raw = (celsius + 20) * 20
        let temp = RawTemperature::from_celsius(0.0);
        assert_eq!(temp.0, 400);

        let temp = RawTemperature::from_celsius(100.0);
        assert_eq!(temp.0, 2400);

        let temp = RawTemperature::from_celsius(23.0);
        assert_eq!(temp.0, 860);
    }

    #[test]
    fn test_raw_temperature_is_valid() {
        assert!(RawTemperature(1000).is_valid());
        assert!(RawTemperature(0).is_valid());
        assert!(!RawTemperature(0x1FFF).is_valid());
        assert!(!RawTemperature::INVALID.is_valid());
    }

    #[test]
    fn test_probe_temperatures_new() {
        let temps = ProbeTemperatures::new();
        for temp in &temps.values {
            assert!(!temp.is_valid());
        }
    }

    #[test]
    fn test_probe_temperatures_from_raw() {
        // Using new formula: celsius = raw * 0.05 - 20
        // raw = 400 -> 0°C, raw = 420 -> 1°C, raw = 2400 -> 100°C
        let raw = [400, 420, 440, 460, 2400, 2420, 2440, 1200];
        let temps = ProbeTemperatures::from_raw(raw);

        assert_eq!(temps.values[0].to_celsius(), Some(0.0));
        assert_eq!(temps.values[1].to_celsius(), Some(1.0));
        assert_eq!(temps.values[4].to_celsius(), Some(100.0));
    }

    #[test]
    fn test_probe_temperatures_sensor() {
        let raw = [400, 420, 440, 460, 2400, 2420, 2440, 1200];
        let temps = ProbeTemperatures::from_raw(raw);

        assert!(temps.sensor(0).is_some());
        assert!(temps.sensor(7).is_some());
        assert!(temps.sensor(8).is_none());
    }

    #[test]
    fn test_virtual_temperatures() {
        let vt = VirtualTemperatures::new(Some(63.0), Some(100.0), Some(200.0));

        assert_eq!(vt.core, Some(63.0));
        assert_eq!(vt.surface, Some(100.0));
        assert_eq!(vt.ambient, Some(200.0));

        // Check Fahrenheit conversions
        assert!((vt.core_fahrenheit().unwrap() - 145.4).abs() < 0.1);
    }

    #[test]
    fn test_packed_bytes_roundtrip() {
        let raw = [1000, 1500, 2000, 2500, 3000, 3500, 4000, 4500];
        let original = ProbeTemperatures::from_raw(raw);

        let packed = original.to_packed_bytes();
        let parsed = ProbeTemperatures::from_packed_bytes(&packed).unwrap();

        for i in 0..8 {
            assert_eq!(
                original.values[i].0, parsed.values[i].0,
                "Mismatch at index {}: expected {}, got {}",
                i, original.values[i].0, parsed.values[i].0
            );
        }
    }
}
