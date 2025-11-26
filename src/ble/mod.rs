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
