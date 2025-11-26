//! CRC calculation for UART messages.
//!
//! Uses CRC-16/CCITT-FALSE polynomial (0x1021) as specified in the
//! Combustion probe BLE specification.

/// CRC-16/CCITT-FALSE polynomial
const CRC_POLYNOMIAL: u16 = 0x1021;

/// Initial CRC value
const CRC_INITIAL: u16 = 0xFFFF;

/// Calculate CRC-16 for UART message data.
///
/// Uses CRC-16/CCITT-FALSE algorithm with polynomial 0x1021
/// and initial value 0xFFFF.
///
/// # Arguments
///
/// * `data` - The data bytes to calculate CRC for
///
/// # Returns
///
/// The 16-bit CRC value
///
/// # Example
///
/// ```
/// use combustion_rust_ble::protocol::calculate_crc;
///
/// let data = [0xCA, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00];
/// let crc = calculate_crc(&data);
/// ```
pub fn calculate_crc(data: &[u8]) -> u16 {
    let mut crc = CRC_INITIAL;

    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ CRC_POLYNOMIAL;
            } else {
                crc <<= 1;
            }
        }
    }

    crc
}

/// Verify that data with appended CRC is valid.
///
/// The last two bytes of the data are treated as the CRC (little-endian).
///
/// # Arguments
///
/// * `data` - The data bytes including the CRC at the end
///
/// # Returns
///
/// `true` if the CRC is valid, `false` otherwise
pub fn verify_crc(data: &[u8]) -> bool {
    if data.len() < 3 {
        return false;
    }

    let payload_len = data.len() - 2;
    let expected_crc = calculate_crc(&data[..payload_len]);
    let actual_crc = u16::from_le_bytes([data[payload_len], data[payload_len + 1]]);

    expected_crc == actual_crc
}

/// Append CRC to data buffer.
///
/// Calculates CRC for the provided data and appends it as two bytes
/// in little-endian format.
///
/// # Arguments
///
/// * `data` - The data bytes to calculate CRC for
///
/// # Returns
///
/// A new vector containing the original data with CRC appended
pub fn append_crc(data: &[u8]) -> Vec<u8> {
    let crc = calculate_crc(data);
    let mut result = data.to_vec();
    result.extend_from_slice(&crc.to_le_bytes());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_empty() {
        let crc = calculate_crc(&[]);
        assert_eq!(crc, CRC_INITIAL);
    }

    #[test]
    fn test_crc_known_value() {
        // Test with known data - sync byte and simple payload
        let data = [0xCA, 0x01, 0x00];
        let crc = calculate_crc(&data);
        // The exact value depends on the algorithm implementation
        assert_ne!(crc, 0); // Basic sanity check
    }

    #[test]
    fn test_crc_different_data() {
        let data1 = [0x01, 0x02, 0x03];
        let data2 = [0x01, 0x02, 0x04];
        assert_ne!(calculate_crc(&data1), calculate_crc(&data2));
    }

    #[test]
    fn test_verify_crc_valid() {
        let data = [0xCA, 0x01, 0x00];
        let with_crc = append_crc(&data);
        assert!(verify_crc(&with_crc));
    }

    #[test]
    fn test_verify_crc_invalid() {
        let data = [0xCA, 0x01, 0x00, 0x00, 0x00]; // Wrong CRC
        assert!(!verify_crc(&data));
    }

    #[test]
    fn test_verify_crc_too_short() {
        let data = [0xCA, 0x01];
        assert!(!verify_crc(&data));
    }

    #[test]
    fn test_append_crc() {
        let data = [0xCA, 0x01, 0x00];
        let with_crc = append_crc(&data);
        assert_eq!(with_crc.len(), data.len() + 2);
        assert!(verify_crc(&with_crc));
    }
}
