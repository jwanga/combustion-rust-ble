//! Real-time temperature monitoring example
//!
//! Run with: cargo run --example temperature_monitor

use combustion_rust_ble::{celsius_to_fahrenheit, DeviceManager, Error, ProbeMode, Result};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (minimal)
    tracing_subscriber::fmt().with_env_filter("warn").init();

    println!("Temperature Monitor");
    println!("==================\n");
    println!("Looking for probes...\n");

    let manager = DeviceManager::new().await?;
    manager.start_scanning().await?;

    // Wait for a probe to be discovered
    tokio::time::sleep(Duration::from_secs(5)).await;

    let probe = manager
        .get_nearest_probe()
        .ok_or_else(|| Error::ProbeNotFound {
            identifier: "any".to_string(),
        })?;

    println!("Found probe: {}", probe.serial_number_string());
    println!("Connecting...\n");

    // Connect to get faster updates
    probe.connect().await?;

    println!("Connected! Monitoring temperatures...");
    println!("Press Ctrl+C to exit.\n");

    // Monitor loop
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nExiting...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                display_temperatures(&probe);
            }
        }
    }

    probe.disconnect().await?;
    manager.shutdown().await?;

    Ok(())
}

fn display_temperatures(probe: &combustion_rust_ble::Probe) {
    // Clear screen and move cursor to top
    print!("\x1B[2J\x1B[1;1H");

    let mode = probe.mode();

    println!("=== Temperature Monitor ===");
    println!(
        "Probe: {} (ID: {})",
        probe.serial_number_string(),
        probe.id()
    );
    println!("Battery: {:?}", probe.battery_status());
    println!("Mode: {:?}", mode);
    println!("Connection: {:?}\n", probe.connection_state());

    // Virtual temperatures
    let vt = probe.virtual_temperatures();
    let sel = &vt.sensor_selection;
    println!("Virtual Sensors:");
    println!("----------------");

    if let Some(core) = vt.core {
        println!(
            "  Core:    {:6.1}°C ({:6.1}°F) [from {}]",
            core,
            celsius_to_fahrenheit(core),
            sel.core_sensor_name()
        );
    } else {
        println!("  Core:    -- [from {}]", sel.core_sensor_name());
    }

    if let Some(surface) = vt.surface {
        println!(
            "  Surface: {:6.1}°C ({:6.1}°F) [from {}]",
            surface,
            celsius_to_fahrenheit(surface),
            sel.surface_sensor_name()
        );
    } else {
        println!("  Surface: -- [from {}]", sel.surface_sensor_name());
    }

    if let Some(ambient) = vt.ambient {
        println!(
            "  Ambient: {:6.1}°C ({:6.1}°F) [from {}]",
            ambient,
            celsius_to_fahrenheit(ambient),
            sel.ambient_sensor_name()
        );
    } else {
        println!("  Ambient: -- [from {}]", sel.ambient_sensor_name());
    }

    // Raw sensor temperatures
    println!("\nRaw Sensors (T1-T8):");
    println!("--------------------");

    let temps = probe.current_temperatures();
    let is_instant_read = mode == ProbeMode::InstantRead;

    for (i, celsius) in temps.to_celsius().iter().enumerate() {
        let sensor_type = match i {
            0 => "Tip (High-Prec)",
            1 => "High-Precision",
            2 => "MCU Sensor",
            3 => "High-Precision",
            4..=6 => "High-Temp",
            7 => "Handle (Ambient)",
            _ => "Unknown",
        };

        // In Instant Read mode, only T1 has valid data - others are set to 0 by the probe
        if is_instant_read && i > 0 {
            println!("  T{}: N/A (Instant Read mode) - {}", i + 1, sensor_type);
        } else if let Some(c) = celsius {
            println!(
                "  T{}: {:6.1}°C ({:6.1}°F) - {}",
                i + 1,
                c,
                celsius_to_fahrenheit(*c),
                sensor_type
            );
        } else {
            println!("  T{}: Invalid", i + 1);
        }
    }

    // Overheating warning
    let overheating = probe.overheating();
    if overheating.is_any_overheating() {
        println!("\n⚠️  WARNING: Sensors overheating!");
        for idx in overheating.overheating_indices() {
            println!("    T{} is overheating!", idx + 1);
        }
    }

    // Prediction info if available
    if let Some(prediction) = probe.prediction_info() {
        println!("\nPrediction:");
        println!("-----------");
        println!("  State: {:?}", prediction.state);
        if prediction.state.is_predicting() {
            let total_secs = prediction.prediction_value_seconds;
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            let secs = total_secs % 60;
            println!("  Target: {:.1}°C", prediction.set_point_temperature);
            println!(
                "  Estimated Core: {:.1}°C",
                prediction.estimated_core_temperature
            );
            println!(
                "  Time Remaining: {:02}:{:02}:{:02} ({} seconds)",
                hours, mins, secs, total_secs
            );
        }
    }

    println!("\nPress Ctrl+C to exit");
    let _ = std::io::stdout().flush();
}
