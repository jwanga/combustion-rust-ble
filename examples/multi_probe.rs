//! Managing multiple probes simultaneously
//!
//! Run with: cargo run --example multi_probe

use combustion_rust_ble::{celsius_to_fahrenheit, DeviceManager, Probe, Result};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("warn").init();

    println!("Multi-Probe Manager");
    println!("===================\n");
    println!("Supports up to 8 probes simultaneously.\n");

    let manager = DeviceManager::new().await?;

    // Track discovered probes
    let probes_found = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let probes_found_clone = probes_found.clone();

    let _handle = manager.on_probe_discovered(move |probe| {
        let count = probes_found_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        println!(
            "Found probe #{}: {} (ID: {}, Color: {:?})",
            count,
            probe.serial_number_string(),
            probe.id(),
            probe.color()
        );
    });

    manager.start_scanning().await?;

    println!("Scanning for probes... (waiting 10 seconds)\n");
    tokio::time::sleep(Duration::from_secs(10)).await;

    let probes: Vec<Arc<Probe>> = manager.probes().values().cloned().collect();

    if probes.is_empty() {
        println!("No probes found!");
        manager.shutdown().await?;
        return Ok(());
    }

    println!("\nFound {} probes. Connecting to all...\n", probes.len());

    // Connect to all probes
    for probe in &probes {
        match probe.connect().await {
            Ok(_) => println!("  ✓ Connected to {}", probe.serial_number_string()),
            Err(e) => println!(
                "  ✗ Failed to connect to {}: {}",
                probe.serial_number_string(),
                e
            ),
        }
    }

    println!("\nMonitoring temperatures... Press Ctrl+C to exit.\n");

    // Display loop
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nExiting...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                display_probe_table(&probes);
            }
        }
    }

    // Disconnect all
    println!("\nDisconnecting from all probes...");
    for probe in &probes {
        let _ = probe.disconnect().await;
    }

    manager.shutdown().await?;
    println!("Done!");

    Ok(())
}

fn display_probe_table(probes: &[Arc<Probe>]) {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");

    println!("=== Multi-Probe Temperature Monitor ===\n");

    // Table header
    println!(
        "{:<12} {:>2} {:>8} {:>8} {:>16} {:>16} {:>16} {:>10}",
        "Probe", "ID", "Color", "Battery", "Core", "Surface", "Ambient", "Sensors"
    );
    println!("{}", "-".repeat(106));

    // Table rows
    for probe in probes {
        let vt = probe.virtual_temperatures();
        let sel = &vt.sensor_selection;

        let core_str = vt
            .core
            .map(|t| format!("{:5.1}°C/{:5.1}°F", t, celsius_to_fahrenheit(t)))
            .unwrap_or_else(|| "     --      ".to_string());

        let surface_str = vt
            .surface
            .map(|t| format!("{:5.1}°C/{:5.1}°F", t, celsius_to_fahrenheit(t)))
            .unwrap_or_else(|| "     --      ".to_string());

        let ambient_str = vt
            .ambient
            .map(|t| format!("{:5.1}°C/{:5.1}°F", t, celsius_to_fahrenheit(t)))
            .unwrap_or_else(|| "     --      ".to_string());

        // Sensor sources
        let sensors_str = format!(
            "{}/{}/{}",
            sel.core_sensor_name(),
            sel.surface_sensor_name(),
            sel.ambient_sensor_name()
        );

        let status = if probe.is_stale() {
            "STALE"
        } else {
            match probe.connection_state() {
                combustion_rust_ble::ConnectionState::Connected => "OK",
                _ => "DISC",
            }
        };

        println!(
            "{:<12} {:>2} {:>8} {:>8} {:>16} {:>16} {:>16} {:>10} [{}]",
            probe.serial_number_string(),
            probe.id(),
            format!("{:?}", probe.color()),
            format!("{:?}", probe.battery_status()),
            core_str,
            surface_str,
            ambient_str,
            sensors_str,
            status
        );
    }

    // Summary
    println!("\n{}", "-".repeat(106));
    println!(
        "Total probes: {} | Active: {}",
        probes.len(),
        probes.iter().filter(|p| !p.is_stale()).count()
    );

    // Find hottest and coldest
    let mut hottest: Option<(String, f64)> = None;
    let mut coldest: Option<(String, f64)> = None;

    for probe in probes {
        if let Some(core) = probe.virtual_temperatures().core {
            let serial = probe.serial_number_string();

            match &hottest {
                None => hottest = Some((serial.clone(), core)),
                Some((_, t)) if core > *t => hottest = Some((serial.clone(), core)),
                _ => {}
            }

            match &coldest {
                None => coldest = Some((serial.clone(), core)),
                Some((_, t)) if core < *t => coldest = Some((serial.clone(), core)),
                _ => {}
            }
        }
    }

    if let Some((serial, temp)) = hottest {
        println!(
            "Hottest: {} at {:.1}°C ({:.1}°F)",
            serial,
            temp,
            celsius_to_fahrenheit(temp)
        );
    }

    if let Some((serial, temp)) = coldest {
        println!(
            "Coldest: {} at {:.1}°C ({:.1}°F)",
            serial,
            temp,
            celsius_to_fahrenheit(temp)
        );
    }

    println!("\nPress Ctrl+C to exit");
    let _ = std::io::stdout().flush();
}
