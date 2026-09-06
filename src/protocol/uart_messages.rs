//! UART message types and parsing.
//!
//! Defines the message format used for communication over the UART service.
//!
//! Message format per the Predictive Probe BLE Specification:
//! - Request: Sync(2) + CRC(2) + MsgType(1) + PayloadLen(1) + Payload
//! - Response: Sync(2) + CRC(2) + MsgType(1) + Success(1) + PayloadLen(1) + Payload

use crate::error::{Error, Result};
use crate::protocol::crc::calculate_crc;

/// UART message sync bytes.
pub const UART_SYNC_BYTES: [u8; 2] = [0xCA, 0xFE];

/// UART message types.
///
/// Message type values per the Predictive Probe BLE Specification:
/// <https://github.com/combustion-inc/combustion-documentation/blob/main/probe_ble_specification.rst>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UartMessageType {
    /// Set probe ID request (0x01).
    SetProbeId = 0x01,
    /// Set probe ID response.
    SetProbeIdResponse = 0x81,

    /// Set probe color request (0x02).
    SetProbeColor = 0x02,
    /// Set probe color response.
    SetProbeColorResponse = 0x82,

    /// Read session information request (0x03).
    ReadSessionInfo = 0x03,
    /// Read session information response.
    ReadSessionInfoResponse = 0x83,

    /// Read temperature logs request (0x04).
    ReadLogs = 0x04,
    /// Read temperature logs response.
    ReadLogsResponse = 0x84,

    /// Set prediction request (0x05).
    SetPrediction = 0x05,
    /// Set prediction response.
    SetPredictionResponse = 0x85,

    /// Read over-temperature request (0x06).
    ReadOverTemperature = 0x06,
    /// Read over-temperature response.
    ReadOverTemperatureResponse = 0x86,

    /// Configure food safety request (0x07).
    ConfigureFoodSafe = 0x07,
    /// Configure food safety response.
    ConfigureFoodSafeResponse = 0x87,

    /// Reset food safety request (0x08).
    ResetFoodSafe = 0x08,
    /// Reset food safety response.
    ResetFoodSafeResponse = 0x88,

    /// Set power mode request (0x09).
    SetPowerMode = 0x09,
    /// Set power mode response.
    SetPowerModeResponse = 0x89,

    /// Reset thermometer request (0x0A).
    ResetThermometer = 0x0A,
    /// Reset thermometer response.
    ResetThermometerResponse = 0x8A,

    /// Set high/low alarms request (0x0B).
    SetHighLowAlarms = 0x0B,
    /// Set high/low alarms response.
    SetHighLowAlarmsResponse = 0x8B,

    /// Silence alarms request (0x0C).
    SilenceAlarms = 0x0C,
    /// Silence alarms response.
    SilenceAlarmsResponse = 0x8C,

    /// Unknown message type.
    Unknown = 0xFF,
}

// Note: Cancel Prediction uses SetPrediction (0x05) with mode=0.
// There's no separate message type for cancellation.

impl UartMessageType {
    /// Create from raw byte value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            0x01 => Self::SetProbeId,
            0x81 => Self::SetProbeIdResponse,
            0x02 => Self::SetProbeColor,
            0x82 => Self::SetProbeColorResponse,
            0x03 => Self::ReadSessionInfo,
            0x83 => Self::ReadSessionInfoResponse,
            0x04 => Self::ReadLogs,
            0x84 => Self::ReadLogsResponse,
            0x05 => Self::SetPrediction, // Also CancelPrediction
            0x85 => Self::SetPredictionResponse,
            0x06 => Self::ReadOverTemperature,
            0x86 => Self::ReadOverTemperatureResponse,
            0x07 => Self::ConfigureFoodSafe,
            0x87 => Self::ConfigureFoodSafeResponse,
            0x08 => Self::ResetFoodSafe,
            0x88 => Self::ResetFoodSafeResponse,
            0x09 => Self::SetPowerMode,
            0x89 => Self::SetPowerModeResponse,
            0x0A => Self::ResetThermometer,
            0x8A => Self::ResetThermometerResponse,
            0x0B => Self::SetHighLowAlarms,
            0x8B => Self::SetHighLowAlarmsResponse,
            0x0C => Self::SilenceAlarms,
            0x8C => Self::SilenceAlarmsResponse,
            _ => Self::Unknown,
        }
    }

    /// Convert to raw byte value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }

    /// Check if this is a response message.
    pub fn is_response(&self) -> bool {
        (*self as u8) & 0x80 != 0
    }

    /// Check if this is a request message.
    pub fn is_request(&self) -> bool {
        !self.is_response()
    }

    /// Get the expected response type for a request.
    pub fn response_type(&self) -> Option<Self> {
        if self.is_response() {
            return None;
        }

        Some(Self::from_raw((*self as u8) | 0x80))
    }
}

/// UART request message header.
///
/// Format: Sync(2) + CRC(2) + MsgType(1) + PayloadLen(1)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UartMessageHeader {
    /// Message type.
    pub message_type: UartMessageType,
    /// Length of payload in bytes.
    pub payload_length: u8,
}

impl UartMessageHeader {
    /// Header size in bytes (sync + CRC + msg_type + payload_len).
    pub const SIZE: usize = 6;
    /// Size of message type + payload length (for CRC calculation).
    pub const CRC_DATA_SIZE: usize = 2;

    /// Create a new header.
    pub fn new(message_type: UartMessageType, payload_length: u8) -> Self {
        Self {
            message_type,
            payload_length,
        }
    }

    /// Parse a header from bytes (request format).
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::InvalidData {
                context: format!("Header too short: {} bytes", data.len()),
            });
        }

        // Check sync bytes
        if data[0] != UART_SYNC_BYTES[0] || data[1] != UART_SYNC_BYTES[1] {
            return Err(Error::InvalidData {
                context: format!("Invalid sync bytes: {:#04x} {:#04x}", data[0], data[1]),
            });
        }

        // CRC is at bytes 2-3 (will be verified separately)
        // Message type is at byte 4
        // Payload length is at byte 5

        Ok(Self {
            message_type: UartMessageType::from_raw(data[4]),
            payload_length: data[5],
        })
    }
}

/// A complete UART message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UartMessage {
    /// Message header.
    pub header: UartMessageHeader,
    /// Message payload.
    pub payload: Vec<u8>,
}

impl UartMessage {
    /// Create a new UART message.
    pub fn new(message_type: UartMessageType, payload: Vec<u8>) -> Self {
        let header = UartMessageHeader::new(message_type, payload.len() as u8);
        Self { header, payload }
    }

    /// Parse a complete UART message from bytes (request format).
    ///
    /// Format: Sync(2) + CRC(2) + MsgType(1) + PayloadLen(1) + Payload
    pub fn parse(data: &[u8]) -> Result<Self> {
        // Need at least header (6 bytes)
        if data.len() < UartMessageHeader::SIZE {
            return Err(Error::InvalidData {
                context: format!("Message too short: {} bytes", data.len()),
            });
        }

        let header = UartMessageHeader::parse(data)?;

        let expected_len = UartMessageHeader::SIZE + header.payload_length as usize;
        if data.len() < expected_len {
            return Err(Error::InvalidData {
                context: format!(
                    "Message incomplete: have {} bytes, need {}",
                    data.len(),
                    expected_len
                ),
            });
        }

        // Extract CRC from bytes 2-3
        let received_crc = u16::from_le_bytes([data[2], data[3]]);

        // Build data for CRC verification: msg_type + payload_len + payload
        let mut crc_data = Vec::with_capacity(2 + header.payload_length as usize);
        crc_data.push(data[4]); // msg_type
        crc_data.push(data[5]); // payload_len
        if header.payload_length > 0 {
            crc_data.extend_from_slice(&data[6..6 + header.payload_length as usize]);
        }

        let calculated_crc = calculate_crc(&crc_data);
        if received_crc != calculated_crc {
            return Err(Error::CrcMismatch {
                expected: calculated_crc,
                actual: received_crc,
            });
        }

        let payload_start = UartMessageHeader::SIZE;
        let payload_end = payload_start + header.payload_length as usize;
        let payload = data[payload_start..payload_end].to_vec();

        Ok(Self { header, payload })
    }

    /// Serialize the message to bytes (request format).
    ///
    /// Format: Sync(2) + CRC(2) + MsgType(1) + PayloadLen(1) + Payload
    pub fn to_bytes(&self) -> Vec<u8> {
        // Build data for CRC: msg_type + payload_len + payload
        let mut crc_data = Vec::with_capacity(2 + self.payload.len());
        crc_data.push(self.header.message_type.to_raw());
        crc_data.push(self.header.payload_length);
        crc_data.extend_from_slice(&self.payload);

        let crc = calculate_crc(&crc_data);

        // Build complete message
        let mut data = Vec::with_capacity(UartMessageHeader::SIZE + self.payload.len());
        data.extend_from_slice(&UART_SYNC_BYTES);
        data.extend_from_slice(&crc.to_le_bytes());
        data.push(self.header.message_type.to_raw());
        data.push(self.header.payload_length);
        data.extend_from_slice(&self.payload);

        data
    }

    /// Get the message type.
    pub fn message_type(&self) -> UartMessageType {
        self.header.message_type
    }

    /// Check if this is a successful response.
    ///
    /// Response messages have a success byte after message type.
    /// Note: Response format is different from request format.
    pub fn is_success(&self) -> bool {
        if !self.header.message_type.is_response() {
            return false;
        }

        // For responses, success is indicated in a different position
        // but since we're primarily sending requests, this is less critical
        self.payload.first().copied().unwrap_or(0) == 0x00
    }
}

// Request builders

/// Build a Read Session Info request.
pub fn build_read_session_info_request() -> UartMessage {
    UartMessage::new(UartMessageType::ReadSessionInfo, vec![])
}

/// Build a Read Logs request.
pub fn build_read_logs_request(min_sequence: u32, max_sequence: u32) -> UartMessage {
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&min_sequence.to_le_bytes());
    payload.extend_from_slice(&max_sequence.to_le_bytes());
    UartMessage::new(UartMessageType::ReadLogs, payload)
}

/// Build a Set Probe ID request.
pub fn build_set_probe_id_request(id: u8) -> UartMessage {
    UartMessage::new(
        UartMessageType::SetProbeId,
        vec![id.saturating_sub(1) & 0x07],
    )
}

/// Build a Set Probe Color request.
pub fn build_set_probe_color_request(color: u8) -> UartMessage {
    UartMessage::new(UartMessageType::SetProbeColor, vec![color & 0x07])
}

/// Build a Set Prediction request.
/// Per spec: 16-bit value with bits 0-9 = set point (raw * 0.1°C), bits 10-11 = mode
pub fn build_set_prediction_request(mode: u8, set_point_raw: u16) -> UartMessage {
    // Bit-pack: lower 10 bits = temperature, bits 10-11 = mode
    let packed: u16 = (set_point_raw & 0x03FF) | (((mode & 0x03) as u16) << 10);
    let payload = packed.to_le_bytes().to_vec();
    UartMessage::new(UartMessageType::SetPrediction, payload)
}

/// Build a Cancel Prediction request.
///
/// Per the spec, cancel prediction uses SetPrediction (0x05) with mode=0.
pub fn build_cancel_prediction_request() -> UartMessage {
    // Mode 0 = cancel prediction, set point doesn't matter
    build_set_prediction_request(0, 0)
}

/// Build a Configure Food Safe request with full 10-byte payload.
///
/// The payload is a packed 10-byte structure containing all food safe parameters.
/// See `FoodSafeConfig::to_bytes()` for the format.
pub fn build_configure_food_safe_request(config_bytes: &[u8; 10]) -> UartMessage {
    UartMessage::new(UartMessageType::ConfigureFoodSafe, config_bytes.to_vec())
}

/// Build a Configure Food Safe request for simplified mode.
///
/// This is a convenience function for simplified mode where only the product type matters.
/// For integrated mode or custom parameters, use `build_configure_food_safe_request` with
/// `FoodSafeConfig::to_bytes()`.
pub fn build_configure_food_safe_simplified_request(product_type: u8) -> UartMessage {
    // For simplified mode, build a minimal config
    // Mode = 0 (Simplified), Product = product_type, Serving = 0 (Immediate)
    // Rest of the fields are zeroed as they're ignored in simplified mode
    let mut payload = [0u8; 10];
    // Byte 0: Mode (bits 0-2) = 0, Product low bits (bits 3-7)
    payload[0] = (product_type as u8 & 0x1F) << 3;
    // Byte 1: Product high bits (bits 0-4), Serving (bits 5-7) = 0
    payload[1] = (product_type >> 5) as u8 & 0x1F;
    UartMessage::new(UartMessageType::ConfigureFoodSafe, payload.to_vec())
}

/// Build a Reset Food Safe request.
pub fn build_reset_food_safe_request() -> UartMessage {
    UartMessage::new(UartMessageType::ResetFoodSafe, vec![])
}

/// Build a Read Over-Temperature request.
pub fn build_read_over_temperature_request() -> UartMessage {
    UartMessage::new(UartMessageType::ReadOverTemperature, vec![])
}

/// Build a Set Power Mode request.
///
/// # Arguments
/// * `power_mode` - Power mode: 0 = Normal (auto power-off in charger), 1 = Always On
pub fn build_set_power_mode_request(power_mode: u8) -> UartMessage {
    UartMessage::new(UartMessageType::SetPowerMode, vec![power_mode & 0x03])
}

/// Build a Reset Thermometer request.
///
/// This resets the thermometer to factory defaults.
pub fn build_reset_thermometer_request() -> UartMessage {
    UartMessage::new(UartMessageType::ResetThermometer, vec![])
}

/// Build a Set High/Low Alarms request.
///
/// # Arguments
/// * `alarm_config` - 44-byte alarm configuration (22 bytes high + 22 bytes low)
///
/// See `AlarmConfig::to_bytes()` for the format.
pub fn build_set_high_low_alarms_request(alarm_config: &[u8; 44]) -> UartMessage {
    UartMessage::new(UartMessageType::SetHighLowAlarms, alarm_config.to_vec())
}

/// Build a Silence Alarms request.
///
/// Silences any currently sounding alarms.
pub fn build_silence_alarms_request() -> UartMessage {
    UartMessage::new(UartMessageType::SilenceAlarms, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_from_raw() {
        assert_eq!(UartMessageType::from_raw(0x01), UartMessageType::SetProbeId);
        assert_eq!(
            UartMessageType::from_raw(0x81),
            UartMessageType::SetProbeIdResponse
        );
        assert_eq!(
            UartMessageType::from_raw(0x03),
            UartMessageType::ReadSessionInfo
        );
        assert_eq!(
            UartMessageType::from_raw(0x83),
            UartMessageType::ReadSessionInfoResponse
        );
        assert_eq!(UartMessageType::from_raw(0xFF), UartMessageType::Unknown);
    }

    #[test]
    fn test_message_type_is_response() {
        assert!(!UartMessageType::SetProbeId.is_response());
        assert!(UartMessageType::SetProbeIdResponse.is_response());
        assert!(!UartMessageType::ReadSessionInfo.is_response());
        assert!(UartMessageType::ReadSessionInfoResponse.is_response());
    }

    #[test]
    fn test_message_type_response_type() {
        assert_eq!(
            UartMessageType::SetProbeId.response_type(),
            Some(UartMessageType::SetProbeIdResponse)
        );
        assert_eq!(
            UartMessageType::ReadSessionInfo.response_type(),
            Some(UartMessageType::ReadSessionInfoResponse)
        );
        assert_eq!(
            UartMessageType::ReadSessionInfoResponse.response_type(),
            None
        );
    }

    #[test]
    fn test_header_parse() {
        // Format: Sync(2) + CRC(2) + MsgType(1) + PayloadLen(1)
        // CRC for [0x01, 0x04] (msg_type=SetProbeId, payload_len=4)
        let crc_data = [0x01, 0x04];
        let crc = calculate_crc(&crc_data);
        let data = [
            0xCA,
            0xFE, // Sync bytes
            crc.to_le_bytes()[0],
            crc.to_le_bytes()[1], // CRC
            0x01,                 // Message type (SetProbeId)
            0x04,                 // Payload length
        ];
        let header = UartMessageHeader::parse(&data).unwrap();
        assert_eq!(header.message_type, UartMessageType::SetProbeId);
        assert_eq!(header.payload_length, 4);
    }

    #[test]
    fn test_header_invalid_sync() {
        let data = [0x00, 0x01, 0x00, 0x00, 0x01, 0x04];
        let result = UartMessageHeader::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_roundtrip() {
        let original = UartMessage::new(UartMessageType::SetProbeId, vec![0x03]);
        let bytes = original.to_bytes();
        let parsed = UartMessage::parse(&bytes).unwrap();

        assert_eq!(original.header.message_type, parsed.header.message_type);
        assert_eq!(original.payload, parsed.payload);
    }

    #[test]
    fn test_message_format() {
        // Test that Set Probe ID message is correctly formatted
        let msg = build_set_probe_id_request(3);
        let bytes = msg.to_bytes();

        // Format: Sync(2) + CRC(2) + MsgType(1) + PayloadLen(1) + Payload
        assert_eq!(bytes[0], 0xCA); // Sync byte 1
        assert_eq!(bytes[1], 0xFE); // Sync byte 2
                                    // bytes[2..4] = CRC
        assert_eq!(bytes[4], 0x01); // Message type (SetProbeId)
        assert_eq!(bytes[5], 0x01); // Payload length
        assert_eq!(bytes[6], 0x02); // Payload (ID 3 -> 0-indexed = 2)

        // Verify total length: 6 header + 1 payload = 7
        assert_eq!(bytes.len(), 7);
    }

    #[test]
    fn test_build_requests() {
        let msg = build_read_session_info_request();
        assert_eq!(msg.message_type(), UartMessageType::ReadSessionInfo);

        let msg = build_read_logs_request(0, 100);
        assert_eq!(msg.message_type(), UartMessageType::ReadLogs);
        assert_eq!(msg.payload.len(), 8);

        let msg = build_set_probe_id_request(3);
        assert_eq!(msg.message_type(), UartMessageType::SetProbeId);
        assert_eq!(msg.payload[0], 2); // 0-indexed
    }

    #[test]
    fn test_new_message_types() {
        // Test SetPowerMode
        assert_eq!(UartMessageType::from_raw(0x09), UartMessageType::SetPowerMode);
        assert_eq!(
            UartMessageType::from_raw(0x89),
            UartMessageType::SetPowerModeResponse
        );

        // Test ResetThermometer
        assert_eq!(
            UartMessageType::from_raw(0x0A),
            UartMessageType::ResetThermometer
        );
        assert_eq!(
            UartMessageType::from_raw(0x8A),
            UartMessageType::ResetThermometerResponse
        );

        // Test SetHighLowAlarms
        assert_eq!(
            UartMessageType::from_raw(0x0B),
            UartMessageType::SetHighLowAlarms
        );
        assert_eq!(
            UartMessageType::from_raw(0x8B),
            UartMessageType::SetHighLowAlarmsResponse
        );

        // Test SilenceAlarms
        assert_eq!(
            UartMessageType::from_raw(0x0C),
            UartMessageType::SilenceAlarms
        );
        assert_eq!(
            UartMessageType::from_raw(0x8C),
            UartMessageType::SilenceAlarmsResponse
        );
    }

    #[test]
    fn test_build_new_requests() {
        // Test SetPowerMode
        let msg = build_set_power_mode_request(1);
        assert_eq!(msg.message_type(), UartMessageType::SetPowerMode);
        assert_eq!(msg.payload.len(), 1);
        assert_eq!(msg.payload[0], 1);

        // Test ResetThermometer
        let msg = build_reset_thermometer_request();
        assert_eq!(msg.message_type(), UartMessageType::ResetThermometer);
        assert!(msg.payload.is_empty());

        // Test SetHighLowAlarms
        let alarm_config = [0u8; 44];
        let msg = build_set_high_low_alarms_request(&alarm_config);
        assert_eq!(msg.message_type(), UartMessageType::SetHighLowAlarms);
        assert_eq!(msg.payload.len(), 44);

        // Test SilenceAlarms
        let msg = build_silence_alarms_request();
        assert_eq!(msg.message_type(), UartMessageType::SilenceAlarms);
        assert!(msg.payload.is_empty());
    }
}
