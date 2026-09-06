//! Example: Hand an application-owned btleplug adapter to the library
//!
//! Use this pattern when your application already talks to other BLE devices
//! through btleplug and should not open a second `Manager`.
//!
//! Run with: cargo run --example existing_adapter

use combustion_rust_ble::btleplug::api::{Central as _, Manager as _};
use combustion_rust_ble::btleplug::platform::Manager;
use combustion_rust_ble::{DeviceManager, Error, Result};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("combustion_rust_ble=info".parse().unwrap()),
        )
        .init();

    // The application owns the btleplug manager and adapter...
    let btle = Manager::new().await?;
    let adapter = btle
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or(Error::BluetoothUnavailable)?;
    println!("Application adapter: {:?}", adapter.adapter_info().await.ok());

    // ...and lends the adapter to combustion-rust-ble.
    let manager = DeviceManager::with_adapter(adapter);
    manager.start_scanning().await?;

    println!("Scanning for 10 seconds...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    for probe in manager.get_probes_by_signal() {
        println!(
            "Found probe {} (RSSI {:?} dBm)",
            probe.serial_number_string(),
            probe.rssi()
        );
    }

    // The same adapter is still available to the application afterwards.
    let _still_ours = manager.adapter();

    manager.shutdown().await?;
    Ok(())
}
