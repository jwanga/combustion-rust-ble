//! Advertising data parsing.
//!
//! Parses manufacturer-specific advertising data from Combustion probes.

use crate::data::{ProbeTemperatures, VirtualSensorSelection, VirtualTemperatures};
use crate::error::{Error, Result};

/// Product type identifier from advertising data.
///
/// Values defined in the MeatNet Node BLE specification:
/// <https://github.com/combustion-inc/combustion-documentation/blob/main/meatnet_node_ble_specification.rst#product-type>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ProductType {
    /// Unknown product type.
    Unknown = 0,
    /// Predictive Thermometer probe.
    PredictiveProbe = 1,
    /// MeatNet Repeater Node.
    MeatNetRepeater = 2,
    /// Giant Grill Gauge.
    GiantGrillGauge = 3,
    /// Display (Timer).
    Display = 4,
    /// Booster (Charger).
    Booster = 5,
}

impl ProductType {
    /// Create from raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            1 => Self::PredictiveProbe,
            2 => Self::MeatNetRepeater,
            3 => Self::GiantGrillGauge,
            4 => Self::Display,
            5 => Self::Booster,
            _ => Self::Unknown,
        }
    }

    /// Check if this is a Predictive Probe.
    pub fn is_predictive_probe(&self) -> bool {
        matches!(self, Self::PredictiveProbe)
    }

    /// Check if this is a probe (alias for is_predictive_probe for backwards compatibility).
    #[deprecated(since = "0.1.0", note = "use is_predictive_probe() instead")]
    pub fn is_probe(&self) -> bool {
        self.is_predictive_probe()
    }
}

/// Probe operational mode from advertising data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ProbeMode {
    /// Normal cooking mode (250ms advertising interval).
    #[default]
    Normal = 0,
    /// Instant read mode with fast updates.
    InstantRead = 1,
    /// Reserved for future use.
    Reserved = 2,
    /// Error state.
    Error = 3,
}

impl ProbeMode {
    /// Create from raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value & 0x03 {
            0 => Self::Normal,
            1 => Self::InstantRead,
            2 => Self::Reserved,
            3 => Self::Error,
            _ => Self::Normal,
        }
    }

    /// Convert to raw byte value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }
}

/// Battery status from advertising data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum BatteryStatus {
    /// Battery is OK.
    #[default]
    Ok = 0,
    /// Battery is low.
    Low = 1,
}

impl BatteryStatus {
    /// Create from raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Ok,
            _ => Self::Low,
        }
    }

    /// Check if battery is low.
    pub fn is_low(&self) -> bool {
        matches!(self, Self::Low)
    }
}

/// Probe ID (1-8) from advertising data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ProbeId(pub u8);

impl ProbeId {
    /// Minimum valid probe ID.
    pub const MIN: u8 = 1;
    /// Maximum valid probe ID.
    pub const MAX: u8 = 8;

    /// Create a new ProbeId, clamping to valid range.
    pub fn new(value: u8) -> Self {
        Self(value.clamp(Self::MIN, Self::MAX))
    }

    /// Create a new ProbeId from raw value (0-indexed internally).
    pub fn from_raw(value: u8) -> Self {
        Self::new((value & 0x07) + 1)
    }

    /// Get the raw 0-indexed value for transmission.
    pub fn to_raw(&self) -> u8 {
        self.0.saturating_sub(1) & 0x07
    }

    /// Get the display value (1-8).
    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for ProbeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Probe color (silicone ring color).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ProbeColor {
    /// Yellow ring.
    #[default]
    Yellow = 0,
    /// Grey ring.
    Grey = 1,
    /// Red ring.
    Red = 2,
    /// Orange ring.
    Orange = 3,
    /// Blue ring.
    Blue = 4,
    /// Green ring.
    Green = 5,
    /// Purple ring.
    Purple = 6,
    /// Pink ring.
    Pink = 7,
}

impl ProbeColor {
    /// Create from raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value & 0x07 {
            0 => Self::Yellow,
            1 => Self::Grey,
            2 => Self::Red,
            3 => Self::Orange,
            4 => Self::Blue,
            5 => Self::Green,
            6 => Self::Purple,
            7 => Self::Pink,
            _ => Self::Yellow,
        }
    }

    /// Convert to raw byte value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }

    /// Get the color name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Yellow => "Yellow",
            Self::Grey => "Grey",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
        }
    }
}

impl std::fmt::Display for ProbeColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Parsed advertising data from a Combustion device.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvertisingData {
    /// Product type (probe, display, etc.).
    pub product_type: ProductType,
    /// Unique serial number.
    pub serial_number: u32,
    /// Temperature readings from all 8 sensors.
    pub temperatures: ProbeTemperatures,
    /// Operational mode.
    pub mode: ProbeMode,
    /// Probe ID (1-8).
    pub probe_id: ProbeId,
    /// Probe color.
    pub color: ProbeColor,
    /// Battery status.
    pub battery_status: BatteryStatus,
    /// Virtual temperatures (core, surface, ambient).
    pub virtual_temperatures: VirtualTemperatures,
    /// Bitmask of overheating sensors.
    pub overheating_sensors: u8,
}

impl AdvertisingData {
    /// Minimum size of advertising data payload.
    const MIN_SIZE: usize = 20;

    /// Parse advertising data from raw bytes.
    ///
    /// # Arguments
    ///
    /// * `data` - Raw manufacturer-specific advertising data
    ///
    /// # Returns
    ///
    /// Parsed advertising data or an error.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < Self::MIN_SIZE {
            return Err(Error::InvalidData {
                context: format!(
                    "Advertising data too short: {} bytes (need at least {})",
                    data.len(),
                    Self::MIN_SIZE
                ),
            });
        }

        // Byte 0: Product type
        let product_type = ProductType::from_raw(data[0]);

        // Bytes 1-4: Serial number (little-endian)
        let serial_number = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);

        // Bytes 5-17: Packed temperatures (13 bytes for 8 x 13-bit values)
        let temperatures = ProbeTemperatures::from_packed_bytes(&data[5..18]).ok_or_else(|| {
            Error::InvalidData {
                context: "Failed to parse packed temperatures".to_string(),
            }
        })?;

        // Byte 18: Mode and ID (packed 8-bit field)
        // Per MeatNet Node BLE spec:
        // - Bits 0-1: Mode (0-3)
        // - Bits 2-4: Color ID (0-7)
        // - Bits 5-7: Probe ID (0-7, representing IDs 1-8)
        let mode_id_byte = data[18];
        let mode = ProbeMode::from_raw(mode_id_byte & 0x03);
        let color = ProbeColor::from_raw((mode_id_byte >> 2) & 0x07);
        let probe_id = ProbeId::from_raw((mode_id_byte >> 5) & 0x07);

        // Byte 19: Battery status and virtual sensor selection
        // Bit 0: Battery status (0 = OK, 1 = Low)
        // Bits 1-7: Virtual sensor selection byte
        let status_byte = data[19];
        let battery_status = BatteryStatus::from_raw(status_byte & 0x01);

        // Parse virtual sensor selection from bits 1-7 and compute virtual temperatures
        let virtual_sensors_byte = status_byte >> 1;
        let virtual_temperatures = Self::compute_virtual_temps(&temperatures, virtual_sensors_byte);

        // Byte 20: Network info (unused)
        // Byte 21: Overheating sensors
        let overheating_sensors = if data.len() >= 22 { data[21] } else { 0 };

        Ok(Self {
            product_type,
            serial_number,
            temperatures,
            mode,
            probe_id,
            color,
            battery_status,
            virtual_temperatures,
            overheating_sensors,
        })
    }

    /// Compute virtual temperatures from raw temperatures and sensor selection byte.
    ///
    /// The virtual sensor selection byte encodes which physical sensor to use:
    /// - Bits 0-2: Virtual Core sensor (selects from T1-T6, values 0-5)
    /// - Bits 3-4: Virtual Surface sensor (selects from T4-T7, values 0-3 map to T4-T7)
    /// - Bits 5-6: Virtual Ambient sensor (selects from T5-T8, values 0-3 map to T5-T8)
    fn compute_virtual_temps(
        temperatures: &ProbeTemperatures,
        virtual_sensors_byte: u8,
    ) -> VirtualTemperatures {
        // Parse the sensor selection
        let sensor_selection = VirtualSensorSelection::from_byte(virtual_sensors_byte);

        // Get temperatures from the selected sensors
        let core = if (sensor_selection.core_sensor as usize) < 6 {
            temperatures.values[sensor_selection.core_sensor as usize].to_celsius()
        } else {
            None
        };

        let surface = if (sensor_selection.surface_sensor as usize) < 8 {
            temperatures.values[sensor_selection.surface_sensor as usize].to_celsius()
        } else {
            None
        };

        let ambient = if (sensor_selection.ambient_sensor as usize) < 8 {
            temperatures.values[sensor_selection.ambient_sensor as usize].to_celsius()
        } else {
            None
        };

        VirtualTemperatures::with_selection(core, surface, ambient, sensor_selection)
    }

    /// Get the serial number as a formatted string.
    pub fn serial_number_string(&self) -> String {
        format!("{:08X}", self.serial_number)
    }

    /// Check if any sensor is overheating.
    pub fn is_any_overheating(&self) -> bool {
        self.overheating_sensors != 0
    }

    /// Check if a specific sensor is overheating.
    pub fn is_sensor_overheating(&self, index: usize) -> bool {
        if index > 7 {
            return false;
        }
        (self.overheating_sensors & (1 << index)) != 0
    }
}

/// Overheating information from advertising or status data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Overheating {
    /// Bitmask of sensors currently overheating (bit 0 = T1, bit 7 = T8).
    pub overheating_sensors: u8,
}

impl Overheating {
    /// Create new overheating info from bitmask.
    pub fn new(sensors: u8) -> Self {
        Self {
            overheating_sensors: sensors,
        }
    }

    /// Check if a specific sensor is overheating.
    ///
    /// # Arguments
    ///
    /// * `sensor_index` - Sensor index (0-7)
    pub fn is_sensor_overheating(&self, sensor_index: usize) -> bool {
        if sensor_index > 7 {
            return false;
        }
        (self.overheating_sensors & (1 << sensor_index)) != 0
    }

    /// Check if any sensor in the "internal" range (T1-T4) is overheating.
    pub fn is_internal_overheating(&self) -> bool {
        (self.overheating_sensors & 0x0F) != 0
    }

    /// Check if any sensor in the "handle" range (T5-T8) is overheating.
    pub fn is_handle_overheating(&self) -> bool {
        (self.overheating_sensors & 0xF0) != 0
    }

    /// Check if any sensor is overheating.
    pub fn is_any_overheating(&self) -> bool {
        self.overheating_sensors != 0
    }

    /// Get list of overheating sensor indices.
    pub fn overheating_indices(&self) -> Vec<usize> {
        (0..8).filter(|i| self.is_sensor_overheating(*i)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_product_type() {
        assert_eq!(ProductType::from_raw(0), ProductType::Unknown);
        assert_eq!(ProductType::from_raw(1), ProductType::PredictiveProbe);
        assert_eq!(ProductType::from_raw(2), ProductType::MeatNetRepeater);
        assert_eq!(ProductType::from_raw(3), ProductType::GiantGrillGauge);
        assert_eq!(ProductType::from_raw(4), ProductType::Display);
        assert_eq!(ProductType::from_raw(5), ProductType::Booster);
        assert_eq!(ProductType::from_raw(99), ProductType::Unknown);

        assert!(ProductType::PredictiveProbe.is_predictive_probe());
        assert!(!ProductType::Display.is_predictive_probe());
        assert!(!ProductType::Booster.is_predictive_probe());
    }

    #[test]
    fn test_probe_mode() {
        assert_eq!(ProbeMode::from_raw(0), ProbeMode::Normal);
        assert_eq!(ProbeMode::from_raw(1), ProbeMode::InstantRead);
        assert_eq!(ProbeMode::from_raw(3), ProbeMode::Error);
    }

    #[test]
    fn test_battery_status() {
        assert_eq!(BatteryStatus::from_raw(0), BatteryStatus::Ok);
        assert!(!BatteryStatus::Ok.is_low());
        assert!(BatteryStatus::Low.is_low());
    }

    #[test]
    fn test_probe_id() {
        let id = ProbeId::new(3);
        assert_eq!(id.as_u8(), 3);

        let id = ProbeId::from_raw(2);
        assert_eq!(id.as_u8(), 3); // 0-indexed + 1

        // Clamping
        assert_eq!(ProbeId::new(0).as_u8(), 1);
        assert_eq!(ProbeId::new(10).as_u8(), 8);
    }

    #[test]
    fn test_probe_color() {
        assert_eq!(ProbeColor::from_raw(0), ProbeColor::Yellow);
        assert_eq!(ProbeColor::from_raw(4), ProbeColor::Blue);
        assert_eq!(ProbeColor::Blue.name(), "Blue");
    }

    #[test]
    fn test_overheating() {
        let overheat = Overheating::new(0b00010001);

        assert!(overheat.is_sensor_overheating(0));
        assert!(!overheat.is_sensor_overheating(1));
        assert!(overheat.is_sensor_overheating(4));
        assert!(overheat.is_any_overheating());
        assert!(overheat.is_internal_overheating());
        assert!(overheat.is_handle_overheating());

        let indices = overheat.overheating_indices();
        assert_eq!(indices, vec![0, 4]);
    }

    #[test]
    fn test_advertising_data_parse() {
        // Create test data with minimum size
        let mut data = vec![0u8; 27];
        data[0] = 1; // Product type: Probe
        data[1] = 0x78; // Serial number (little-endian)
        data[2] = 0x56;
        data[3] = 0x34;
        data[4] = 0x12;
        // Temperatures (13 bytes) - set some values
        for i in 5..18 {
            data[i] = 0x00;
        }
        // Byte 18: Mode/Color/ID packed byte
        // Bits 0-1: Mode (0 = Normal)
        // Bits 2-4: Color (1 = Grey)
        // Bits 5-7: Probe ID (2 = ID 3)
        data[18] = 0b01000100; // Mode=0, Color=1 (Grey), ID=2 (displays as 3)
        data[19] = 0x00; // Battery OK

        let parsed = AdvertisingData::parse(&data).unwrap();
        assert_eq!(parsed.product_type, ProductType::PredictiveProbe);
        assert_eq!(parsed.serial_number, 0x12345678);
        assert_eq!(parsed.mode, ProbeMode::Normal);
        assert_eq!(parsed.color, ProbeColor::Grey);
        assert_eq!(parsed.probe_id.as_u8(), 3);
        assert_eq!(parsed.battery_status, BatteryStatus::Ok);
    }

    #[test]
    fn test_advertising_data_too_short() {
        let data = vec![0u8; 10];
        let result = AdvertisingData::parse(&data);
        assert!(result.is_err());
    }
}
