//! Debug example for Combustion probe - outputs all data to terminal for debugging
//!
//! Run with: cargo run --example probe_debug
//!
//! This example outputs all probe data to the terminal in a readable format,
//! useful for debugging BLE communication and data parsing issues.

use combustion_rust_ble::{celsius_to_fahrenheit, DeviceManager, PredictionMode, Result};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with trace level for combustion_rust_ble to see all notifications
    tracing_subscriber::fmt()
        .with_env_filter("combustion_rust_ble=trace,btleplug=warn")
        .init();

    println!("===========================================");
    println!("  Combustion Probe Debug Tool");
    println!("===========================================\n");

    let manager = DeviceManager::new().await?;
    println!("[INFO] Device manager created");

    println!("[INFO] Starting BLE scan...");
    manager.start_scanning().await?;

    println!("[INFO] Waiting 5 seconds for probe discovery...\n");
    tokio::time::sleep(Duration::from_secs(5)).await;

    let probes = manager.probes();
    println!("[INFO] Found {} probe(s)\n", probes.len());

    if probes.is_empty() {
        println!("[WARN] No probes found. Make sure your probe is powered on and nearby.");
        manager.shutdown().await?;
        return Ok(());
    }

    // Get the first probe
    let probe = probes.values().next().unwrap().clone();

    println!("===========================================");
    println!("  Probe Information (from advertising)");
    println!("===========================================");
    print_probe_info(&probe);

    println!("\n[INFO] Connecting to probe...");
    match probe.connect().await {
        Ok(_) => println!("[INFO] Connected successfully!"),
        Err(e) => {
            println!("[ERROR] Failed to connect: {:?}", e);
            manager.shutdown().await?;
            return Ok(());
        }
    }

    // Wait a moment for notifications to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("\n[INFO] Setting prediction target to 63.0°C with TimeToRemoval mode...");
    match probe
        .set_prediction(PredictionMode::TimeToRemoval, 63.0)
        .await
    {
        Ok(_) => println!("[INFO] Prediction set successfully!"),
        Err(e) => println!("[ERROR] Failed to set prediction: {:?}", e),
    }

    println!("\n[INFO] Monitoring probe data. Press Ctrl+C to exit.\n");

    // Main monitoring loop
    let mut iteration = 0;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\n[INFO] Ctrl+C received, shutting down...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                iteration += 1;
                println!("\n===========================================");
                println!("  Update #{} - Connection: {:?}", iteration, probe.connection_state());
                println!("===========================================");

                print_probe_info(&probe);
                print_temperatures(&probe);
                print_prediction_info(&probe);
                print_food_safety_info(&probe);
                print_log_info(&probe);

                let _ = std::io::stdout().flush();
            }
        }
    }

    println!("\n[INFO] Cancelling prediction...");
    let _ = probe.cancel_prediction().await;

    println!("[INFO] Disconnecting...");
    let _ = probe.disconnect().await;

    println!("[INFO] Shutting down...");
    manager.shutdown().await?;

    println!("[INFO] Done!");
    Ok(())
}

fn print_probe_info(probe: &combustion_rust_ble::Probe) {
    println!("\n--- Probe Identity ---");
    println!("  Serial Number: {}", probe.serial_number_string());
    println!("  ID: {}", probe.id());
    println!("  Color: {:?}", probe.color());
    println!("  Mode: {:?}", probe.mode());
    println!("  Battery: {:?}", probe.battery_status());
    println!("  RSSI: {:?} dBm", probe.rssi());
    println!("  Connection State: {:?}", probe.connection_state());
    println!("  Is Stale: {}", probe.is_stale());
}

fn print_temperatures(probe: &combustion_rust_ble::Probe) {
    println!("\n--- Temperatures ---");

    // Virtual temperatures
    let vt = probe.virtual_temperatures();
    let sel = &vt.sensor_selection;

    println!("  Virtual Sensors:");
    if let Some(core) = vt.core {
        println!(
            "    Core [{}]: {:.1}°C ({:.1}°F)",
            sel.core_sensor_name(),
            core,
            celsius_to_fahrenheit(core)
        );
    } else {
        println!("    Core [{}]: N/A", sel.core_sensor_name());
    }

    if let Some(surface) = vt.surface {
        println!(
            "    Surface [{}]: {:.1}°C ({:.1}°F)",
            sel.surface_sensor_name(),
            surface,
            celsius_to_fahrenheit(surface)
        );
    } else {
        println!("    Surface [{}]: N/A", sel.surface_sensor_name());
    }

    if let Some(ambient) = vt.ambient {
        println!(
            "    Ambient [{}]: {:.1}°C ({:.1}°F)",
            sel.ambient_sensor_name(),
            ambient,
            celsius_to_fahrenheit(ambient)
        );
    } else {
        println!("    Ambient [{}]: N/A", sel.ambient_sensor_name());
    }

    // Raw temperatures
    println!("  Raw Sensors (T1-T8):");
    let temps = probe.current_temperatures();
    let celsius_temps = temps.to_celsius();
    let sensor_names = [
        "T1 (Tip)",
        "T2",
        "T3 (MCU)",
        "T4",
        "T5",
        "T6",
        "T7",
        "T8 (Handle)",
    ];

    for (i, celsius) in celsius_temps.iter().enumerate() {
        if let Some(c) = celsius {
            println!(
                "    {}: {:.1}°C ({:.1}°F)",
                sensor_names[i],
                c,
                celsius_to_fahrenheit(*c)
            );
        } else {
            println!("    {}: Invalid", sensor_names[i]);
        }
    }

    // Overheating
    let overheating = probe.overheating();
    if overheating.is_any_overheating() {
        println!("  ⚠ OVERHEAT WARNING: {:?}", overheating);
    } else {
        println!("  Overheating: None");
    }
}

fn print_prediction_info(probe: &combustion_rust_ble::Probe) {
    println!("\n--- Prediction Status ---");

    if let Some(info) = probe.prediction_info() {
        println!("  State: {:?}", info.state);
        println!("  Mode: {:?}", info.mode);
        println!("  Type: {:?}", info.prediction_type);
        println!(
            "  Setpoint: {:.1}°C ({:.1}°F)",
            info.set_point_temperature,
            celsius_to_fahrenheit(info.set_point_temperature)
        );
        println!(
            "  Heat Start: {:.1}°C ({:.1}°F)",
            info.heat_start_temperature,
            celsius_to_fahrenheit(info.heat_start_temperature)
        );
        println!(
            "  Estimated Core: {:.1}°C ({:.1}°F)",
            info.estimated_core_temperature,
            celsius_to_fahrenheit(info.estimated_core_temperature)
        );
        let total_secs = info.prediction_value_seconds;
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        println!(
            "  Prediction Time: {:02}:{:02}:{:02} ({} seconds)",
            hours, mins, secs, total_secs
        );
        println!("  Core Sensor Index: {}", info.core_sensor_index);
        println!(
            "  Seconds Since Start: {}",
            info.seconds_since_prediction_start
        );

        // Additional computed values
        if let Some(progress) = info.temperature_progress() {
            println!("  Temperature Progress: {:.1}%", progress);
        }
        println!("  Is Active: {}", info.is_active());
        println!("  Is Complete: {}", info.is_complete());
    } else {
        println!(
            "  (No prediction info available - probe may not be sending status notifications)"
        );
        println!("  This could indicate:");
        println!("    - Probe is not connected");
        println!("    - Status characteristic not subscribed");
        println!("    - Status notifications not being received");
    }
}

fn print_food_safety_info(probe: &combustion_rust_ble::Probe) {
    println!("\n--- Food Safety ---");

    if let Some(data) = probe.food_safe_data() {
        println!("  Product: {:?}", data.product);
        println!("  Serving State: {:?}", data.serving_state);
        println!("  Log Reduction: {:.2}", data.log_reduction);
        println!("  Progress: {:.1}%", data.progress_percent());
        println!(
            "  Seconds Above Threshold: {}",
            data.seconds_above_threshold
        );
        println!("  Is Safe: {}", data.is_safe());
    } else {
        println!("  (Not configured)");
    }
}

fn print_log_info(probe: &combustion_rust_ble::Probe) {
    println!("\n--- Log Sync ---");
    println!("  Min Sequence: {}", probe.min_sequence_number());
    println!("  Max Sequence: {}", probe.max_sequence_number());
    println!("  Percent Synced: {:.1}%", probe.percent_of_logs_synced());

    let log = probe.temperature_log();
    println!("  Log entries: {}", log.len());

    // Session info requires async call, skip for now
    println!("  Session Info: (use read_session_info() async method)");
}
