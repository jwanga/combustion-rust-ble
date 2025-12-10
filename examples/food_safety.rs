//! Food safety (SafeCook) monitoring example
//!
//! Demonstrates the Food Safe features including:
//! - Simplified mode (USDA temperature thresholds)
//! - Integrated mode (time-temperature integration with log reduction)
//! - Configure Food Safe command (0x07)
//! - Reset Food Safe command (0x08)
//! - Food Safe Status monitoring
//!
//! Run with: cargo run --example food_safety
//!
//! To connect to a specific probe:
//!   cargo run --example food_safety -- --serial 1001192D

use combustion_rust_ble::{
    celsius_to_fahrenheit, DeviceManager, Error, FoodSafeConfig, FoodSafeMode,
    FoodSafeState, IntegratedProduct, Result, Serving, SimplifiedProduct,
};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("warn,combustion_rust_ble=debug")
        .init();

    println!("Food Safety Monitor (SafeCook)");
    println!("==============================\n");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let target_serial = args
        .iter()
        .position(|arg| arg == "--serial")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_uppercase());

    println!("=== Food Safe Modes ===");
    println!("1. Simplified Mode - Uses USDA instant temperature thresholds");
    println!("2. Integrated Mode - Uses time-temperature integration for log reduction\n");

    println!("=== Simplified Mode Products ===");
    println!("  - Any Poultry (165°F/74°C)");
    println!("  - Beef/Pork/Veal/Lamb Cuts (145°F/63°C + rest)");
    println!("  - Ground Meats (160°F/71°C)");
    println!("  - Ham Fresh/Smoked (145°F/63°C)");
    println!("  - Ham Cooked/Reheated (165°F/74°C)");
    println!("  - Eggs (160°F/71°C)");
    println!("  - Fish/Shellfish (145°F/63°C)");
    println!("  - Leftovers/Casseroles (165°F/74°C)\n");

    println!("=== Integrated Mode Products ===");
    println!("  - Poultry: 7.0 log reduction");
    println!("  - Meats: 5.0 log reduction");
    println!("  - Ground/Chopped Meats: 6.5 log reduction");
    println!("  - Seafood: 6.0 log reduction");
    println!("  - Dairy: 5.0 log reduction");
    println!("  - Eggs: 5.0 log reduction\n");

    // Choose mode for demonstration
    println!("Choose mode for this session:");
    println!("  1 = Simplified (Chicken - 165°F instant)");
    println!("  2 = Integrated (Chicken - 7.0 log reduction)");
    println!("  3 = Custom Integrated (custom parameters)");
    println!();

    // Default to Integrated mode for demonstration
    let mode = 2;

    let (config, mode_name) = match mode {
        1 => {
            println!("Using SIMPLIFIED mode: Chicken (Any Poultry)");
            (
                FoodSafeConfig::simplified(SimplifiedProduct::AnyPoultry, Serving::ServedImmediately),
                "Simplified - Any Poultry",
            )
        }
        2 => {
            println!("Using INTEGRATED mode: Poultry (7.0 log reduction)");
            (
                FoodSafeConfig::integrated(IntegratedProduct::Poultry, Serving::ServedImmediately),
                "Integrated - Poultry",
            )
        }
        _ => {
            println!("Using CUSTOM INTEGRATED mode");
            (
                FoodSafeConfig::custom(
                    54.4, // ~130°F threshold
                    5.5,  // Z-value
                    70.0, // Reference temp
                    1.0,  // D-value
                    7.0,  // Target log reduction
                    Serving::ServedImmediately,
                ),
                "Custom Integrated",
            )
        }
    };

    println!("\nConfiguration:");
    println!("  Mode: {:?}", config.mode);
    println!("  Threshold: {:.1}°C ({:.1}°F)", config.threshold_temperature, celsius_to_fahrenheit(config.threshold_temperature));
    if config.mode == FoodSafeMode::Integrated {
        println!("  Z-value: {:.1}°C ({:.1}°F)", config.z_value, celsius_to_fahrenheit(config.z_value));
        println!("  Reference Temp: {:.1}°C ({:.1}°F)", config.reference_temperature, celsius_to_fahrenheit(config.reference_temperature));
        println!("  D-value at RT: {:.1}", config.d_value_at_reference);
        println!("  Target Log Reduction: {:.1}", config.target_log_reduction);
    }
    println!("  Serving: {:?}", config.serving);
    println!();

    if let Some(ref serial) = target_serial {
        println!("Looking for probe with serial: {}...\n", serial);
    } else {
        println!("Looking for any available probe...\n");
    }

    let manager = DeviceManager::new().await?;
    manager.start_scanning().await?;

    // Wait for probes to be discovered
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Find the target probe or nearest
    let probe = if let Some(ref serial) = target_serial {
        // Look for probe with matching serial number
        let probes = manager.probes();
        let found = probes.iter().find(|(_, p)| {
            p.serial_number_string().to_uppercase() == *serial
        });
        found
            .map(|(_, p)| p.clone())
            .ok_or_else(|| Error::ProbeNotFound {
                identifier: serial.clone(),
            })?
    } else {
        manager
            .get_nearest_probe()
            .ok_or_else(|| Error::ProbeNotFound {
                identifier: "any".to_string(),
            })?
    };

    println!("Found probe: {}", probe.serial_number_string());
    println!("Connecting...\n");

    probe.connect().await?;

    // Configure food safety with the chosen config
    println!("Configuring food safety ({})...", mode_name);
    probe.configure_food_safe_with_config(config.clone()).await?;

    println!("Food safety monitoring active!\n");
    println!("Insert probe into food and begin cooking.");
    println!("Press Ctrl+C to exit.\n");

    let start_time = std::time::Instant::now();
    let mut safe_notified = false;
    let mut impossible_notified = false;

    // Monitor loop
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nExiting...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                display_food_safety_status(
                    &probe,
                    &config,
                    mode_name,
                    start_time.elapsed(),
                    &mut safe_notified,
                    &mut impossible_notified,
                );
            }
        }
    }

    // Reset food safety
    println!("\nResetting food safety...");
    let _ = probe.reset_food_safe().await;

    probe.disconnect().await?;
    manager.shutdown().await?;

    println!("Done!");

    Ok(())
}

fn display_food_safety_status(
    probe: &combustion_rust_ble::Probe,
    config: &FoodSafeConfig,
    mode_name: &str,
    elapsed: Duration,
    safe_notified: &mut bool,
    impossible_notified: &mut bool,
) {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");

    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║            Food Safety Monitor (SafeCook)                     ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    println!("Probe: {}", probe.serial_number_string());
    println!("Mode: {}", mode_name);
    println!(
        "Elapsed: {:02}:{:02}:{:02}",
        elapsed.as_secs() / 3600,
        (elapsed.as_secs() % 3600) / 60,
        elapsed.as_secs() % 60
    );
    println!();

    let vt = probe.virtual_temperatures();
    let sel = &vt.sensor_selection;

    // Temperature display
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Current Temperatures                                        │");
    println!("├─────────────────────────────────────────────────────────────┤");

    if let Some(core) = vt.core {
        let above = if config.mode == FoodSafeMode::Integrated {
            core >= config.threshold_temperature
        } else {
            core >= config.threshold_temperature
        };
        let status_char = if above { "▲" } else { "▼" };
        println!(
            "│  Core:    {:6.1}°C ({:6.1}°F) [{}] {}                     │",
            core,
            celsius_to_fahrenheit(core),
            sel.core_sensor_name(),
            status_char
        );
    } else {
        println!("│  Core:    --°C (--°F) [{}]                               │", sel.core_sensor_name());
    }

    if let Some(surface) = vt.surface {
        println!(
            "│  Surface: {:6.1}°C ({:6.1}°F) [{}]                         │",
            surface,
            celsius_to_fahrenheit(surface),
            sel.surface_sensor_name()
        );
    }

    if let Some(ambient) = vt.ambient {
        println!(
            "│  Ambient: {:6.1}°C ({:6.1}°F) [{}]                         │",
            ambient,
            celsius_to_fahrenheit(ambient),
            sel.ambient_sensor_name()
        );
    }
    println!("└─────────────────────────────────────────────────────────────┘\n");

    // Food safety status
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Food Safety Status                                          │");
    println!("├─────────────────────────────────────────────────────────────┤");

    if let Some(data) = probe.food_safe_data() {
        let target_reduction = config.target_log_reduction;

        // State display
        let state = data.state();
        let state_icon = match state {
            FoodSafeState::NotSafe => "⏳",
            FoodSafeState::Safe => "✅",
            FoodSafeState::SafetyImpossible => "❌",
        };
        println!("│  State: {} {:?}                                        │", state_icon, state);

        if config.mode == FoodSafeMode::Integrated {
            // Progress bar for integrated mode
            let progress = data.progress_percent();
            let bar_width: usize = 30;
            let filled = ((progress / 100.0) * bar_width as f64) as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

            println!(
                "│  Log Reduction: {:.2} / {:.1} ({:.1}%)                      │",
                data.log_reduction, target_reduction, progress
            );
            println!("│  Progress: {}                          │", bar);
        }

        println!(
            "│  Time at Temp: {} seconds                                 │",
            data.seconds_above_threshold
        );

        // Safety messaging
        match state {
            FoodSafeState::Safe => {
                println!("│                                                             │");
                println!("│  ✅ SAFE TO SERVE - Food has reached safe criteria!         │");
                if !*safe_notified {
                    *safe_notified = true;
                }
            }
            FoodSafeState::SafetyImpossible => {
                println!("│                                                             │");
                println!("│  ❌ SAFETY IMPOSSIBLE - Temperature dropped below threshold │");
                println!("│     Reset and restart for accurate safety monitoring        │");
                if !*impossible_notified {
                    *impossible_notified = true;
                }
            }
            FoodSafeState::NotSafe => {
                println!("│                                                             │");
                println!("│  ⏳ NOT YET SAFE - Continue cooking                         │");

                if let Some(core) = vt.core {
                    if config.mode == FoodSafeMode::Simplified {
                        if core < config.threshold_temperature {
                            let diff = config.threshold_temperature - core;
                            println!(
                                "│  Target: {:.1}°C ({:.1}°F) - Need {:.1}°C ({:.1}°F) more  │",
                                config.threshold_temperature,
                                celsius_to_fahrenheit(config.threshold_temperature),
                                diff,
                                celsius_to_fahrenheit(diff)
                            );
                        } else {
                            println!("│  Temperature reached! Waiting for safety confirmation...  │");
                        }
                    } else if core < config.threshold_temperature {
                        println!(
                            "│  Heat to above {:.1}°C ({:.1}°F) to begin integration     │",
                            config.threshold_temperature,
                            celsius_to_fahrenheit(config.threshold_temperature)
                        );
                    }
                }
            }
        }
    } else {
        println!("│  Waiting for food safety data...                           │");
    }

    println!("└─────────────────────────────────────────────────────────────┘\n");

    // Configuration details
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Configuration                                               │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│  Mode: {:?}                                           │", config.mode);
    println!(
        "│  Threshold: {:.1}°C ({:.1}°F)                              │",
        config.threshold_temperature,
        celsius_to_fahrenheit(config.threshold_temperature)
    );
    if config.mode == FoodSafeMode::Integrated {
        println!("│  Z-value: {:.1}°C ({:.1}°F)                                  │", config.z_value, celsius_to_fahrenheit(config.z_value));
        println!("│  D-value @ {:.0}°C ({:.0}°F): {:.1}s                           │", config.reference_temperature, celsius_to_fahrenheit(config.reference_temperature), config.d_value_at_reference);
        println!("│  Target: {:.1} log reduction                                │", config.target_log_reduction);
    }
    println!("│  Serving: {:?}                                   │", config.serving);
    println!("└─────────────────────────────────────────────────────────────┘\n");

    println!("Press Ctrl+C to exit");
    let _ = std::io::stdout().flush();
}
