// Allow holding locks across await points - we use parking_lot which is designed for this
#![allow(clippy::await_holding_lock)]
// Allow derivable impls for clarity
#![allow(clippy::derivable_impls)]
// Allow unusual byte groupings for UUIDs which have standard format
#![allow(clippy::unusual_byte_groupings)]

//! # combustion-rust-ble
//!
//! A cross-platform Rust library for communicating with Combustion Inc's
//! Predictive Thermometer probes via Bluetooth Low Energy.
//!
//! This library specifically targets **Predictive Probes** (ProductType 1).
//! Other Combustion devices (Display, Booster, MeatNet Repeater, Giant Grill Gauge)
//! are intentionally ignored during discovery.
//!
//! ## Features
//!
//! - **Probe Discovery**: Automatically discover nearby Predictive Probes
//! - **Real-time Temperatures**: Read all 8 sensors in real-time
//! - **Virtual Sensors**: Computed Core, Surface, and Ambient temperatures
//! - **Temperature Logging**: Download complete temperature history
//! - **Prediction Engine**: Set target temperatures and get time predictions
//! - **Food Safety**: SafeCook/USDA Safe compliance monitoring
//! - **Multi-probe Support**: Manage up to 8 probes simultaneously
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use combustion_rust_ble::{DeviceManager, Result};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Create device manager and start scanning
//!     let manager = DeviceManager::new().await?;
//!     manager.start_scanning().await?;
//!
//!     // Wait for probes to be discovered
//!     tokio::time::sleep(std::time::Duration::from_secs(5)).await;
//!
//!     // Get all discovered probes
//!     for (id, probe) in manager.probes() {
//!         println!("Found probe: {} ({})", probe.serial_number_string(), id);
//!
//!         // Read current temperatures
//!         let temps = probe.current_temperatures();
//!         let virtual_temps = probe.virtual_temperatures();
//!
//!         if let Some(core) = virtual_temps.core {
//!             println!("  Core temperature: {:.1}°C", core);
//!         }
//!     }
//!
//!     manager.shutdown().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Platform Notes
//!
//! ### macOS
//! Requires Bluetooth permission. Add `NSBluetoothAlwaysUsageDescription`
//! to your Info.plist for bundled apps.
//!
//! ### Linux
//! Requires BlueZ. User may need to be in the `bluetooth` group.
//!
//! ### Windows
//! Requires Windows 10 or later with Bluetooth LE support.
//!
//! ## Feature Flags
//!
//! - `serde`: Enable serialization/deserialization for data types

// Public modules
pub mod ble;
pub mod data;
pub mod device_manager;
pub mod error;
pub mod probe;
pub mod protocol;
pub mod utils;

// Re-exports for convenience
pub use device_manager::{DeviceManager, MAX_PROBES};
pub use error::{Error, Result};
pub use probe::{CallbackHandle, Probe};
pub use utils::{celsius_to_fahrenheit, fahrenheit_to_celsius};

// Re-export commonly used types from submodules
pub use ble::advertising::{BatteryStatus, Overheating, ProbeColor, ProbeId, ProbeMode};
pub use ble::connection::ConnectionState;
pub use data::{
    FoodSafeConfig, FoodSafeData, FoodSafeMode, FoodSafeProduct, FoodSafeServingState,
    FoodSafeState, FoodSafeStatus, IntegratedProduct, LoggedDataPoint, PredictionInfo,
    PredictionLog, PredictionMode, PredictionState, PredictionType, ProbeTemperatures,
    RawTemperature, Serving, SessionInfo, SimplifiedProduct, TemperatureLog, VirtualSensorSelection,
    VirtualTemperatures,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_exports() {
        // Verify that key types are exported
        let _ = std::any::TypeId::of::<DeviceManager>();
        let _ = std::any::TypeId::of::<Probe>();
        let _ = std::any::TypeId::of::<Error>();
        let _ = std::any::TypeId::of::<ProbeTemperatures>();
        let _ = std::any::TypeId::of::<VirtualTemperatures>();
        let _ = std::any::TypeId::of::<PredictionInfo>();
        let _ = std::any::TypeId::of::<FoodSafeData>();
    }

    #[test]
    fn test_temperature_conversion() {
        assert!((celsius_to_fahrenheit(100.0) - 212.0).abs() < 0.001);
        assert!((fahrenheit_to_celsius(212.0) - 100.0).abs() < 0.001);
    }
}
