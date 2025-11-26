//! Download temperature logs from a probe
//!
//! Run with: cargo run --example log_download

use combustion_rust_ble::{DeviceManager, Error, Result};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("warn").init();

    println!("Temperature Log Download");
    println!("========================\n");
    println!("Looking for probes...\n");

    let manager = DeviceManager::new().await?;
    manager.start_scanning().await?;

    tokio::time::sleep(Duration::from_secs(5)).await;

    let probe = manager
        .get_nearest_probe()
        .ok_or_else(|| Error::ProbeNotFound {
            identifier: "any".to_string(),
        })?;

    println!("Found probe: {}", probe.serial_number_string());
    println!("Connecting...\n");

    probe.connect().await?;

    println!("Connected!");
    println!(
        "Log range: {} - {}",
        probe.min_sequence_number(),
        probe.max_sequence_number()
    );

    let total_logs = probe
        .max_sequence_number()
        .saturating_sub(probe.min_sequence_number())
        + 1;
    println!("Total logs available: {}\n", total_logs);

    if total_logs == 0 {
        println!("No logs available on this probe.");
        probe.disconnect().await?;
        manager.shutdown().await?;
        return Ok(());
    }

    println!("Downloading logs...\n");

    // Monitor download progress
    let start_time = std::time::Instant::now();
    let mut last_percent = 0.0;

    loop {
        let percent = probe.percent_of_logs_synced();

        if (percent - last_percent).abs() > 0.1 {
            print!("\rProgress: {:5.1}%", percent);
            let _ = std::io::stdout().flush();
            last_percent = percent;
        }

        if percent >= 100.0 {
            break;
        }

        // Timeout after 2 minutes
        if start_time.elapsed() > Duration::from_secs(120) {
            println!("\n\nDownload timed out.");
            break;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let elapsed = start_time.elapsed();
    println!("\n\nDownload complete in {:.1}s!", elapsed.as_secs_f64());

    // Get the log
    let log = probe.temperature_log();

    println!("\nLog Summary:");
    println!("------------");
    println!("  Session ID: {:08X}", log.session_id);
    println!("  Sample Period: {}ms", log.sample_period_ms);
    println!("  Data Points: {}", log.len());
    println!(
        "  Duration: {:.1} minutes",
        log.duration().as_secs_f64() / 60.0
    );

    // Show some statistics
    if !log.data_points.is_empty() {
        let mut min_temp = f64::MAX;
        let mut max_temp = f64::MIN;

        for point in &log.data_points {
            for temp in &point.temperatures.values {
                if let Some(t) = temp.to_celsius() {
                    min_temp = min_temp.min(t);
                    max_temp = max_temp.max(t);
                }
            }
        }

        if min_temp != f64::MAX {
            println!("\nTemperature Range:");
            println!(
                "  Min: {:.1}°C ({:.1}°F)",
                min_temp,
                combustion_rust_ble::celsius_to_fahrenheit(min_temp)
            );
            println!(
                "  Max: {:.1}°C ({:.1}°F)",
                max_temp,
                combustion_rust_ble::celsius_to_fahrenheit(max_temp)
            );
        }
    }

    // Export to CSV
    let csv = log.to_csv();
    let filename = format!("probe_{}_log.csv", probe.serial_number_string());

    match std::fs::write(&filename, &csv) {
        Ok(_) => {
            println!("\nSaved to: {}", filename);
            println!("CSV size: {} bytes", csv.len());
        }
        Err(e) => {
            println!("\nFailed to save CSV: {}", e);
            println!("\nCSV Preview (first 500 chars):");
            println!("{}", &csv[..csv.len().min(500)]);
        }
    }

    probe.disconnect().await?;
    manager.shutdown().await?;

    println!("\nDone!");

    Ok(())
}
