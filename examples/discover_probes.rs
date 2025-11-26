//! Basic example: Discover all nearby Combustion probes
//!
//! Run with: cargo run --example discover_probes

use combustion_rust_ble::{celsius_to_fahrenheit, DeviceManager, Result};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("combustion_rust_ble=debug".parse().unwrap()),
        )
        .init();

    println!("Starting Combustion probe discovery...");
    println!("Make sure your probe is out of the charger!\n");

    let manager = DeviceManager::new().await?;

    // Register callback for discovered probes
    let _handle = manager.on_probe_discovered(|probe| {
        println!("\nDiscovered probe:");
        println!("  Serial: {}", probe.serial_number_string());
        println!("  ID: {}", probe.id());
        println!("  Color: {:?}", probe.color());
        println!("  RSSI: {:?} dBm", probe.rssi());
        println!("  Battery: {:?}", probe.battery_status());
        println!("  Mode: {:?}", probe.mode());

        // Show current temperatures
        let temps = probe.current_temperatures();
        let celsius = temps.to_celsius();
        println!("  Temperatures (T1-T8):");
        for (i, temp) in celsius.iter().enumerate() {
            if let Some(t) = temp {
                println!(
                    "    T{}: {:.1}°C ({:.1}°F)",
                    i + 1,
                    t,
                    celsius_to_fahrenheit(*t)
                );
            } else {
                println!("    T{}: Invalid", i + 1);
            }
        }

        // Show virtual temperatures
        let virtual_temps = probe.virtual_temperatures();
        let sel = &virtual_temps.sensor_selection;
        println!("  Virtual Temperatures:");
        if let Some(core) = virtual_temps.core {
            println!(
                "    Core: {:.1}°C ({:.1}°F) [from {}]",
                core,
                celsius_to_fahrenheit(core),
                sel.core_sensor_name()
            );
        }
        if let Some(surface) = virtual_temps.surface {
            println!(
                "    Surface: {:.1}°C ({:.1}°F) [from {}]",
                surface,
                celsius_to_fahrenheit(surface),
                sel.surface_sensor_name()
            );
        }
        if let Some(ambient) = virtual_temps.ambient {
            println!(
                "    Ambient: {:.1}°C ({:.1}°F) [from {}]",
                ambient,
                celsius_to_fahrenheit(ambient),
                sel.ambient_sensor_name()
            );
        }
    });

    // Register callback for stale probes
    let _stale_handle = manager.on_probe_stale(|probe| {
        println!(
            "\nProbe {} went stale (no data for 15s)",
            probe.serial_number_string()
        );
    });

    manager.start_scanning().await?;

    println!("Scanning for 30 seconds...");
    println!("Press Ctrl+C to exit early.\n");

    // Scan for 30 seconds
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        _ = tokio::signal::ctrl_c() => {
            println!("\nInterrupted!");
        }
    }

    println!("\n--- Scan Complete ---");
    println!("Total probes found: {}", manager.probe_count());

    // List all probes
    for (id, probe) in manager.probes() {
        println!(
            "  {} - {} (RSSI: {:?})",
            probe.serial_number_string(),
            id,
            probe.rssi()
        );
    }

    manager.shutdown().await?;
    println!("\nDone!");

    Ok(())
}
