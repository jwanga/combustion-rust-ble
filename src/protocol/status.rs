//! Probe status parsing.
//!
//! Parses status notifications from the probe status characteristic.

use crate::ble::advertising::{BatteryStatus, Overheating, ProbeColor, ProbeId, ProbeMode};
use crate::data::{
    PredictionInfo, PredictionMode, PredictionState, PredictionType, ProbeTemperatures,
    VirtualSensorSelection, VirtualTemperatures,
};
use crate::error::{Error, Result};

/// Parsed probe status from status characteristic notifications.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeStatus {
    /// Minimum sequence number available on probe.
    pub min_sequence_number: u32,
    /// Maximum sequence number available on probe.
    pub max_sequence_number: u32,
    /// Temperature readings from all 8 sensors.
    pub temperatures: ProbeTemperatures,
    /// Probe operational mode.
    pub mode: ProbeMode,
    /// Probe ID (1-8).
    pub probe_id: ProbeId,
    /// Probe color.
    pub color: ProbeColor,
    /// Battery status.
    pub battery_status: BatteryStatus,
    /// Virtual temperatures.
    pub virtual_temperatures: VirtualTemperatures,
    /// Prediction information.
    pub prediction: Option<PredictionInfo>,
    /// Overheating information.
    pub overheating: Overheating,
}

impl ProbeStatus {
    /// Minimum size of status data (through prediction status at byte 29).
    const MIN_SIZE: usize = 30;

    /// Parse probe status from notification data.
    ///
    /// Based on the official Combustion Probe BLE specification, the status packet layout is:
    /// - Bytes 0-3: Min Sequence Number (uint32_t little-endian)
    /// - Bytes 4-7: Max Sequence Number (uint32_t little-endian)
    /// - Bytes 8-20: Raw Temperature Data (13 bytes, 8 × 13-bit packed)
    /// - Byte 21: Mode/ID (bits 0-1: Mode, bits 2-4: Color, bits 5-7: Probe ID)
    /// - Byte 22: Battery & Virtual Sensors (bit 0: Battery, bits 1-7: Virtual sensor config)
    /// - Bytes 23-29: Prediction Status (7 bytes, 56 bits)
    /// - Bytes 30-39: Food Safe Data (10 bytes) - optional
    /// - Bytes 40-47: Food Safe Status (8 bytes) - optional
    /// - Byte 48: Overheating Sensors - optional
    /// - Byte 49: Thermometer Preferences - optional
    /// - Bytes 50-71: High Alarm Status array (22 bytes) - optional
    /// - Bytes 72-93: Low Alarm Status array (22 bytes) - optional
    pub fn parse(data: &[u8]) -> Result<Self> {
        use tracing::debug;

        debug!(
            "ProbeStatus::parse called with {} bytes: {:02X?}",
            data.len(),
            data
        );

        if data.len() < Self::MIN_SIZE {
            return Err(Error::InvalidData {
                context: format!(
                    "Status data too short: {} bytes (need at least {})",
                    data.len(),
                    Self::MIN_SIZE
                ),
            });
        }

        // Bytes 0-3: Min sequence number (little-endian)
        let min_sequence_number = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

        // Bytes 4-7: Max sequence number (little-endian)
        let max_sequence_number = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        // Bytes 8-20: Packed temperatures (13 bytes for 8 x 13-bit values)
        let temperatures = ProbeTemperatures::from_packed_bytes(&data[8..21]).ok_or_else(|| {
            Error::InvalidData {
                context: "Failed to parse packed temperatures".to_string(),
            }
        })?;

        // Byte 21: Mode and ID (packed 8-bit field)
        // Per Combustion Probe BLE spec:
        // - Bits 0-1: Mode (0-3)
        // - Bits 2-4: Color ID (0-7)
        // - Bits 5-7: Probe ID (0-7, representing IDs 1-8)
        let mode_id_byte = data[21];
        let mode = ProbeMode::from_raw(mode_id_byte & 0x03);
        let color = ProbeColor::from_raw((mode_id_byte >> 2) & 0x07);
        let probe_id = ProbeId::from_raw((mode_id_byte >> 5) & 0x07);

        // Byte 22: Battery and Virtual Sensors
        // - Bit 0: Battery status (0=OK, 1=Low)
        // - Bits 1-7: Virtual sensor configuration
        let battery_status = BatteryStatus::from_raw(data[22] & 0x01);

        // Virtual sensors are encoded in byte 22 bits 1-7 and use temperature data
        let virtual_temperatures = Self::parse_virtual_temps_from_config(data[22], &temperatures);

        // Bytes 23-29: Prediction Status (7 bytes)
        debug!(
            "Parsing prediction from bytes 23-29: {:02X?}",
            &data[23..30]
        );
        let prediction = Self::parse_prediction_status(&data[23..30]);
        debug!("Parsed prediction: {:?}", prediction);

        // Byte 48: Overheating Sensors (if available)
        let overheating = if data.len() > 48 {
            Overheating::new(data[48])
        } else {
            Overheating::default()
        };

        Ok(Self {
            min_sequence_number,
            max_sequence_number,
            temperatures,
            mode,
            probe_id,
            color,
            battery_status,
            virtual_temperatures,
            prediction,
            overheating,
        })
    }

    /// Parse virtual temperatures from configuration byte and raw temperatures.
    ///
    /// The virtual sensor configuration is in byte 22 bits 1-7:
    /// - Bits 1-3: Virtual Core sensor index (0=T1 through 5=T6)
    /// - Bits 4-5: Virtual Surface sensor offset (0=T4, 1=T5, 2=T6, 3=T7)
    /// - Bits 6-7: Virtual Ambient sensor offset (0=T5, 1=T6, 2=T7, 3=T8)
    fn parse_virtual_temps_from_config(
        config_byte: u8,
        temperatures: &ProbeTemperatures,
    ) -> VirtualTemperatures {
        // Extract virtual sensor indices
        let core_index = ((config_byte >> 1) & 0x07) as usize;
        let surface_offset = ((config_byte >> 4) & 0x03) as usize;
        let ambient_offset = ((config_byte >> 6) & 0x03) as usize;

        // Convert offsets to actual sensor indices
        let surface_index = 3 + surface_offset; // T4=3, T5=4, T6=5, T7=6
        let ambient_index = 4 + ambient_offset; // T5=4, T6=5, T7=6, T8=7

        // Get temperatures (indices are 0-based)
        let core = if core_index < 8 {
            temperatures.to_celsius().get(core_index).copied().flatten()
        } else {
            None
        };

        let surface = if surface_index < 8 {
            temperatures
                .to_celsius()
                .get(surface_index)
                .copied()
                .flatten()
        } else {
            None
        };

        let ambient = if ambient_index < 8 {
            temperatures
                .to_celsius()
                .get(ambient_index)
                .copied()
                .flatten()
        } else {
            None
        };

        // Create sensor selection with the actual indices used
        let sensor_selection =
            VirtualSensorSelection::new(core_index as u8, surface_index as u8, ambient_index as u8);

        VirtualTemperatures::with_selection(core, surface, ambient, sensor_selection)
    }

    /// Parse prediction status from 7-byte packed structure.
    ///
    /// Prediction Status is a 7-byte (56-bit) packed structure:
    /// - Bits 0-3: Prediction State (4 bits)
    /// - Bits 4-5: Prediction Mode (2 bits)
    /// - Bits 6-7: Prediction Type (2 bits)
    /// - Bits 8-17: Set Point Temperature (10 bits, value * 0.1°C)
    /// - Bits 18-27: Heat Start Temperature (10 bits, value * 0.1°C)
    /// - Bits 28-44: Prediction Value Seconds (17 bits)
    /// - Bits 45-55: Estimated Core Temperature (11 bits, (value * 0.1°C) - 20°C)
    fn parse_prediction_status(data: &[u8]) -> Option<PredictionInfo> {
        use tracing::debug;

        if data.len() < 7 {
            debug!(
                "Not enough bytes for prediction status (have {}, need 7)",
                data.len()
            );
            return None;
        }

        // Byte 0: State (bits 0-3), Mode (bits 4-5), Type (bits 6-7)
        let state = PredictionState::from_raw(data[0] & 0x0F);
        let mode = PredictionMode::from_raw((data[0] >> 4) & 0x03);
        let prediction_type = PredictionType::from_raw((data[0] >> 6) & 0x03);

        // Bytes 1-2: Set Point Temperature (10 bits starting at bit 8)
        // Bits 8-17: lower 8 bits in byte 1, upper 2 bits in lower bits of byte 2
        let set_point_raw = (data[1] as u16) | ((data[2] as u16 & 0x03) << 8);
        let set_point_temperature = set_point_raw as f64 * 0.1;

        // Bytes 2-3: Heat Start Temperature (10 bits starting at bit 18)
        // Bits 18-27: bits 2-7 of byte 2, bits 0-3 of byte 3
        let heat_start_raw = ((data[2] as u16) >> 2) | ((data[3] as u16 & 0x0F) << 6);
        let heat_start_temperature = heat_start_raw as f64 * 0.1;

        // Bytes 3-5: Prediction Value Seconds (17 bits starting at bit 28)
        // Bits 28-44: bits 4-7 of byte 3, all of byte 4, bits 0-4 of byte 5
        let prediction_value_seconds =
            ((data[3] as u32) >> 4) | ((data[4] as u32) << 4) | ((data[5] as u32 & 0x1F) << 12);

        // Bytes 5-6: Estimated Core Temperature (11 bits starting at bit 45)
        // Bits 45-55: bits 5-7 of byte 5, all of byte 6
        let estimated_core_raw = ((data[5] as u16) >> 5) | ((data[6] as u16) << 3);
        let estimated_core_temperature = (estimated_core_raw as f64 * 0.1) - 20.0;

        // Core sensor index is not in the 7-byte prediction status
        let core_sensor_index = 0;

        debug!(
            "Parsed prediction: state={:?}, mode={:?}, type={:?}, setpoint={:.1}°C, heat_start={:.1}°C, pred_secs={}, est_core={:.1}°C",
            state, mode, prediction_type, set_point_temperature, heat_start_temperature, prediction_value_seconds, estimated_core_temperature
        );

        Some(PredictionInfo {
            state,
            mode,
            prediction_type,
            set_point_temperature,
            heat_start_temperature,
            prediction_value_seconds,
            estimated_core_temperature,
            seconds_since_prediction_start: 0, // Not in status notification
            core_sensor_index,
        })
    }

    /// Get the number of log entries available on the probe.
    pub fn available_log_count(&self) -> u32 {
        self.max_sequence_number
            .saturating_sub(self.min_sequence_number)
            + 1
    }

    /// Check if the probe has logs available.
    pub fn has_logs(&self) -> bool {
        self.max_sequence_number >= self.min_sequence_number
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_status_data() -> Vec<u8> {
        // Minimum size is 30 bytes (through prediction status)
        let mut data = vec![0u8; 50];

        // Bytes 0-3: Min sequence: 10
        data[0..4].copy_from_slice(&10u32.to_le_bytes());

        // Bytes 4-7: Max sequence: 100
        data[4..8].copy_from_slice(&100u32.to_le_bytes());

        // Bytes 8-20: Packed temperatures (13 bytes) - all zeros (will parse to valid temps)

        // Byte 21: Mode/ID byte - Mode 0, Color 1, ID 0
        data[21] = 0b00000100; // bits 0-1: mode=0, bits 2-4: color=1, bits 5-7: id=0

        // Byte 22: Battery & Virtual Sensors
        data[22] = 0x00; // Battery OK, virtual sensors use defaults

        // Bytes 23-29: Prediction Status (7 bytes)
        // State=Predicting (3), Mode=TimeToRemoval (1), Type=Removal (1)
        // Byte 0: (type << 6) | (mode << 4) | state = (1 << 6) | (1 << 4) | 3 = 0x53
        data[23] = 0x53;
        // Setpoint = 63.0°C = 630 raw (0x276)
        // Bytes 1-2: lower 8 bits in byte 1, upper 2 bits in byte 2
        data[24] = 0x76; // lower 8 bits of 630
        data[25] = (0x02 << 0) | (0x00 << 2); // upper 2 bits of 630, then heat_start bits 0-5
                                              // Heat start = 20.0°C = 200 raw (0xC8)
        data[25] |= (200 & 0x3F) << 2; // bits 2-7 of heat_start
        data[26] = ((200 >> 6) & 0x0F) as u8; // bits 0-3: remaining heat_start bits
                                              // Prediction seconds = 300 (5 minutes)
        data[26] |= ((300 & 0x0F) << 4) as u8; // bits 4-7: lower 4 bits of pred_secs
        data[27] = ((300 >> 4) & 0xFF) as u8; // bits 0-7: next 8 bits
        data[28] = ((300 >> 12) & 0x1F) as u8; // bits 0-4: upper 5 bits
                                               // Estimated core = 45.0°C = (45 + 20) * 10 = 650 raw
        data[28] |= ((650 & 0x07) << 5) as u8; // bits 5-7: lower 3 bits
        data[29] = ((650 >> 3) & 0xFF) as u8; // remaining 8 bits

        // Byte 48: Overheating sensors
        data[48] = 0x00;

        data
    }

    #[test]
    fn test_probe_status_parse() {
        let data = create_test_status_data();
        let status = ProbeStatus::parse(&data).unwrap();

        assert_eq!(status.min_sequence_number, 10);
        assert_eq!(status.max_sequence_number, 100);
        assert_eq!(status.mode, ProbeMode::Normal);
        assert_eq!(status.battery_status, BatteryStatus::Ok);

        // Check prediction was parsed
        let prediction = status.prediction.expect("prediction should be present");
        assert_eq!(prediction.state, PredictionState::Predicting);
        assert_eq!(prediction.mode, PredictionMode::TimeToRemoval);
        assert_eq!(prediction.prediction_type, PredictionType::Removal);
    }

    #[test]
    fn test_probe_status_too_short() {
        let data = vec![0u8; 10];
        let result = ProbeStatus::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_available_log_count() {
        let data = create_test_status_data();
        let status = ProbeStatus::parse(&data).unwrap();

        assert_eq!(status.available_log_count(), 91);
        assert!(status.has_logs());
    }
}
