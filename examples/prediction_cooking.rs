//! Prediction cooking example - set target temp and monitor countdown
//!
//! Run with: cargo run --example prediction_cooking

use combustion_rust_ble::{celsius_to_fahrenheit, DeviceManager, Error, PredictionMode, Result};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("warn").init();

    println!("Prediction Cooking Example");
    println!("==========================\n");

    // Target temperature: 63°C (145°F) for medium-rare beef
    let target_celsius = 63.0;

    println!(
        "Target: {:.1}°C ({:.1}°F) - Medium-rare beef",
        target_celsius,
        celsius_to_fahrenheit(target_celsius)
    );
    println!("\nLooking for probes...\n");

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

    // Set up prediction
    println!(
        "Setting target temperature to {:.1}°C ({:.1}°F)...",
        target_celsius,
        celsius_to_fahrenheit(target_celsius)
    );

    probe
        .set_prediction(PredictionMode::TimeToRemoval, target_celsius)
        .await?;

    println!("Prediction set! Monitoring...\n");
    println!("Press Ctrl+C to exit.\n");

    // Monitor loop
    let mut done_notified = false;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nExiting...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                let vt = probe.virtual_temperatures();
                let sel = &vt.sensor_selection;
                let core_temp = vt.core.unwrap_or(0.0);
                let core_sensor = sel.core_sensor_name();

                if let Some(info) = probe.prediction_info() {
                    match info.state {
                        combustion_rust_ble::PredictionState::Predicting => {
                            let total_secs = info.prediction_value_seconds;
                            let hours = total_secs / 3600;
                            let mins = (total_secs % 3600) / 60;
                            let secs = total_secs % 60;
                            print!(
                                "\rCore [{}]: {:5.1}°C ({:5.1}°F) | Target: {:5.1}°C | Ready in: {:02}:{:02}:{:02} ({} sec)  ",
                                core_sensor,
                                info.estimated_core_temperature,
                                celsius_to_fahrenheit(info.estimated_core_temperature),
                                info.set_point_temperature,
                                hours,
                                mins,
                                secs,
                                total_secs
                            );
                            let _ = std::io::stdout().flush();
                        }
                        combustion_rust_ble::PredictionState::RemovalPredictionDone => {
                            if !done_notified {
                                println!("\n\n🎉 TARGET REACHED! Remove from heat now!");
                                println!("Final core temperature [{}]: {:.1}°C ({:.1}°F)",
                                    core_sensor, core_temp, celsius_to_fahrenheit(core_temp));
                                done_notified = true;
                            }
                        }
                        combustion_rust_ble::PredictionState::ProbeNotInserted => {
                            print!("\rWaiting for probe insertion...                                    ");
                            let _ = std::io::stdout().flush();
                        }
                        combustion_rust_ble::PredictionState::ProbeInserted => {
                            print!("\rProbe inserted, waiting for cooking to start...                  ");
                            let _ = std::io::stdout().flush();
                        }
                        combustion_rust_ble::PredictionState::Warming => {
                            print!(
                                "\rCore [{}]: {:5.1}°C ({:5.1}°F) | Warming up, gathering data...         ",
                                core_sensor, core_temp, celsius_to_fahrenheit(core_temp)
                            );
                            let _ = std::io::stdout().flush();
                        }
                        _ => {
                            print!(
                                "\rCore [{}]: {:5.1}°C ({:5.1}°F) | State: {:?}                           ",
                                core_sensor, core_temp, celsius_to_fahrenheit(core_temp), info.state
                            );
                            let _ = std::io::stdout().flush();
                        }
                    }
                } else {
                    print!(
                        "\rCore [{}]: {:5.1}°C ({:5.1}°F) | No prediction data                       ",
                        core_sensor, core_temp, celsius_to_fahrenheit(core_temp)
                    );
                    let _ = std::io::stdout().flush();
                }
            }
        }
    }

    // Cancel prediction and cleanup
    println!("\nCancelling prediction...");
    let _ = probe.cancel_prediction().await;

    probe.disconnect().await?;
    manager.shutdown().await?;

    println!("Done!");

    Ok(())
}
