//! BLE communication module.
//!
//! This module provides low-level Bluetooth Low Energy functionality
//! for discovering and communicating with Combustion probes.

pub mod advertising;
pub mod characteristics;
pub mod connection;
pub mod scanner;
pub mod uuids;

pub use advertising::{AdvertisingData, ProductType};
pub use characteristics::CharacteristicHandler;
pub use connection::{ConnectionManager, ConnectionState};
pub use scanner::BleScanner;
pub use uuids::*;

/// True if `err`'s display string contains any of `markers`.
///
/// btleplug flattens BlueZ D-Bus errors to their message text (the D-Bus error
/// name is dropped), so substring matching is the only way to classify them.
pub(crate) fn btleplug_error_matches(err: &btleplug::Error, markers: &[&str]) -> bool {
    let msg = err.to_string();
    markers.iter().any(|m| msg.contains(m))
}
