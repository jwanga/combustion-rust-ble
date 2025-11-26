//! BLE Service and Characteristic UUIDs.
//!
//! Contains all UUID constants used for Combustion probe communication.

use uuid::Uuid;

// Device Information Service (Standard BLE)
/// Standard BLE Device Information Service UUID.
pub const DEVICE_INFO_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_180a_0000_1000_8000_00805f9b34fb);
/// Manufacturer Name characteristic UUID.
pub const MANUFACTURER_NAME_UUID: Uuid = Uuid::from_u128(0x0000_2a29_0000_1000_8000_00805f9b34fb);
/// Model Number characteristic UUID.
pub const MODEL_NUMBER_UUID: Uuid = Uuid::from_u128(0x0000_2a24_0000_1000_8000_00805f9b34fb);
/// Serial Number characteristic UUID.
pub const SERIAL_NUMBER_UUID: Uuid = Uuid::from_u128(0x0000_2a25_0000_1000_8000_00805f9b34fb);
/// Hardware Revision characteristic UUID.
pub const HARDWARE_REVISION_UUID: Uuid = Uuid::from_u128(0x0000_2a27_0000_1000_8000_00805f9b34fb);
/// Firmware Revision characteristic UUID.
pub const FIRMWARE_REVISION_UUID: Uuid = Uuid::from_u128(0x0000_2a26_0000_1000_8000_00805f9b34fb);

// Probe Status Service (Combustion Custom)
/// Combustion Probe Status Service UUID.
pub const PROBE_STATUS_SERVICE_UUID: Uuid =
    Uuid::from_u128(0x0000_0100_caab_3792_3d44_97ae51c1407a);
/// Combustion Probe Status Characteristic UUID (Read, Notify).
pub const PROBE_STATUS_CHARACTERISTIC_UUID: Uuid =
    Uuid::from_u128(0x0000_0101_caab_3792_3d44_97ae51c1407a);

// UART Service (Nordic NUS - Nordic UART Service)
/// Nordic UART Service UUID.
pub const UART_SERVICE_UUID: Uuid = Uuid::from_u128(0x6e40_0001_b5a3_f393_e0a9_e50e24dcca9e);
/// UART RX characteristic UUID (write to probe).
pub const UART_RX_UUID: Uuid = Uuid::from_u128(0x6e40_0002_b5a3_f393_e0a9_e50e24dcca9e);
/// UART TX characteristic UUID (notifications from probe).
pub const UART_TX_UUID: Uuid = Uuid::from_u128(0x6e40_0003_b5a3_f393_e0a9_e50e24dcca9e);

// DFU Service (Nordic Buttonless DFU)
/// Nordic DFU Service UUID for firmware updates.
pub const DFU_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_fe59_0000_1000_8000_00805f9b34fb);

// Combustion manufacturer ID for advertising data
/// Combustion Inc's Bluetooth manufacturer ID.
pub const COMBUSTION_MANUFACTURER_ID: u16 = 0x09C7;

/// Check if a service UUID is a Combustion-specific service.
pub fn is_combustion_service(uuid: &Uuid) -> bool {
    *uuid == PROBE_STATUS_SERVICE_UUID || *uuid == UART_SERVICE_UUID
}

/// Check if a service UUID indicates a Combustion probe.
pub fn is_probe_service(uuid: &Uuid) -> bool {
    *uuid == PROBE_STATUS_SERVICE_UUID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_format() {
        // Verify UUIDs are properly formatted
        let device_info = DEVICE_INFO_SERVICE_UUID.to_string();
        assert!(device_info.contains("180a"));

        let probe_status = PROBE_STATUS_SERVICE_UUID.to_string();
        assert!(probe_status.contains("caab"));
    }

    #[test]
    fn test_is_combustion_service() {
        assert!(is_combustion_service(&PROBE_STATUS_SERVICE_UUID));
        assert!(is_combustion_service(&UART_SERVICE_UUID));
        assert!(!is_combustion_service(&DEVICE_INFO_SERVICE_UUID));
    }

    #[test]
    fn test_is_probe_service() {
        assert!(is_probe_service(&PROBE_STATUS_SERVICE_UUID));
        assert!(!is_probe_service(&UART_SERVICE_UUID));
    }

    #[test]
    fn test_probe_status_characteristic_uuid() {
        // Print the actual UUID value for debugging
        println!(
            "PROBE_STATUS_CHARACTERISTIC_UUID: {}",
            PROBE_STATUS_CHARACTERISTIC_UUID
        );
        println!("PROBE_STATUS_SERVICE_UUID: {}", PROBE_STATUS_SERVICE_UUID);
        println!("UART_TX_UUID: {}", UART_TX_UUID);

        // Verify the characteristic UUID is in the expected format
        let uuid_str = PROBE_STATUS_CHARACTERISTIC_UUID.to_string();
        assert!(
            uuid_str.contains("0101"),
            "UUID should contain '0101': {}",
            uuid_str
        );
        assert!(
            uuid_str.contains("caab"),
            "UUID should contain 'caab': {}",
            uuid_str
        );
    }
}
