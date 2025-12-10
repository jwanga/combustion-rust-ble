//! Temperature alarm control example
//!
//! Demonstrates the temperature alarm features including:
//! - Setting high and low temperature alarms
//! - Monitoring alarm status
//! - Silencing alarms
//! - Power mode control
//!
//! Run with: cargo run --example alarm_control
//!
//! To connect to a specific probe:
//!   cargo run --example alarm_control -- --serial 1001192D

use combustion_rust_ble::{celsius_to_fahrenheit, DeviceManager, Error, PowerMode, Result};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("warn,combustion_rust_ble=debug")
        .init();

    println!("Temperature Alarm Control");
    println!("========================\n");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let target_serial = args
        .iter()
        .position(|arg| arg == "--serial")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_uppercase());

    println!("=== Alarm Features ===");
    println!("- High/Low temperature alarms for all 8 sensors");
    println!("- Virtual sensor alarms (Core, Surface, Ambient)");
    println!("- Audible alarm when threshold is crossed");
    println!("- Alarm silencing");
    println!();

    println!("=== Power Modes ===");
    println!("- Normal: Probe auto power-off when in charger");
    println!("- Always On: Probe stays powered in charger");
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
        let probes = manager.probes();
        let found = probes
            .iter()
            .find(|(_, p)| p.serial_number_string().to_uppercase() == *serial);
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

    // Display menu
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║                Temperature Alarm Control                       ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    println!("Commands:");
    println!("  1 = Set core high alarm (74°C / 165°F - poultry safe temp)");
    println!("  2 = Set core high alarm (63°C / 145°F - beef/pork safe temp)");
    println!("  3 = Set core low alarm (4°C / 40°F - refrigeration temp)");
    println!("  4 = Set surface high alarm (200°C / 392°F - grill surface)");
    println!("  5 = Disable all alarms");
    println!("  6 = Silence alarms");
    println!("  7 = Set power mode to Normal");
    println!("  8 = Set power mode to Always On");
    println!("  9 = Reset thermometer to factory defaults");
    println!("  m = Monitor mode (show live status)");
    println!("  q = Quit");
    println!();

    // Simple command loop
    let mut input = String::new();
    loop {
        print!("Enter command: ");
        std::io::stdout().flush().unwrap();
        input.clear();

        if std::io::stdin().read_line(&mut input).is_err() {
            continue;
        }

        let cmd = input.trim().to_lowercase();

        match cmd.as_str() {
            "1" => {
                println!("Setting core high alarm to 74°C (165°F)...");
                match probe.set_core_high_alarm(74.0).await {
                    Ok(_) => println!("Alarm set successfully!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "2" => {
                println!("Setting core high alarm to 63°C (145°F)...");
                match probe.set_core_high_alarm(63.0).await {
                    Ok(_) => println!("Alarm set successfully!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "3" => {
                println!("Setting core low alarm to 4°C (40°F)...");
                match probe.set_core_low_alarm(4.0).await {
                    Ok(_) => println!("Alarm set successfully!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "4" => {
                println!("Setting surface high alarm to 200°C (392°F)...");
                let mut config = probe.alarm_config().unwrap_or_default();
                config.set_surface_high_alarm(200.0, true);
                match probe.set_alarms(&config).await {
                    Ok(_) => println!("Alarm set successfully!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "5" => {
                println!("Disabling all alarms...");
                match probe.disable_all_alarms().await {
                    Ok(_) => println!("All alarms disabled!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "6" => {
                println!("Silencing alarms...");
                match probe.silence_alarms().await {
                    Ok(_) => println!("Alarms silenced!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "7" => {
                println!("Setting power mode to Normal...");
                match probe.set_power_mode(PowerMode::Normal).await {
                    Ok(_) => println!("Power mode set to Normal!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "8" => {
                println!("Setting power mode to Always On...");
                match probe.set_power_mode(PowerMode::AlwaysOn).await {
                    Ok(_) => println!("Power mode set to Always On!"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }
            "9" => {
                println!("Resetting thermometer to factory defaults...");
                println!("WARNING: This will reset probe ID, color, alarms, etc.");
                print!("Are you sure? (y/n): ");
                std::io::stdout().flush().unwrap();
                let mut confirm = String::new();
                if std::io::stdin().read_line(&mut confirm).is_ok()
                    && confirm.trim().to_lowercase() == "y"
                {
                    match probe.reset_thermometer().await {
                        Ok(_) => println!("Thermometer reset!"),
                        Err(e) => println!("Error: {:?}", e),
                    }
                } else {
                    println!("Reset cancelled.");
                }
            }
            "m" => {
                println!("\nEntering monitor mode. Press Ctrl+C to return to menu.\n");
                monitor_mode(&probe).await;
            }
            "q" | "quit" | "exit" => {
                println!("Exiting...");
                break;
            }
            "" => {}
            _ => {
                println!("Unknown command: {}", cmd);
            }
        }
        println!();
    }

    probe.disconnect().await?;
    manager.shutdown().await?;

    println!("Done!");
    Ok(())
}

async fn monitor_mode(probe: &combustion_rust_ble::Probe) {
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nReturning to menu...\n");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                display_status(probe);
            }
        }
    }
}

fn display_status(probe: &combustion_rust_ble::Probe) {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");

    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║              Temperature Alarm Monitor                         ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    println!("Probe: {}", probe.serial_number_string());
    println!("ID: {} | Color: {}", probe.id().as_u8(), probe.color().name());
    println!("Battery: {:?}", probe.battery_status());
    println!();

    // Power mode
    if let Some(mode) = probe.power_mode() {
        println!("Power Mode: {}", mode.name());
    } else {
        println!("Power Mode: (unknown)");
    }
    println!();

    // Temperatures
    let vt = probe.virtual_temperatures();
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Current Temperatures                                        │");
    println!("├─────────────────────────────────────────────────────────────┤");

    if let Some(core) = vt.core {
        println!(
            "│  Core:    {:6.1}°C ({:6.1}°F)                               │",
            core,
            celsius_to_fahrenheit(core)
        );
    }
    if let Some(surface) = vt.surface {
        println!(
            "│  Surface: {:6.1}°C ({:6.1}°F)                               │",
            surface,
            celsius_to_fahrenheit(surface)
        );
    }
    if let Some(ambient) = vt.ambient {
        println!(
            "│  Ambient: {:6.1}°C ({:6.1}°F)                               │",
            ambient,
            celsius_to_fahrenheit(ambient)
        );
    }
    println!("└─────────────────────────────────────────────────────────────┘\n");

    // Alarm status
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Alarm Status                                                │");
    println!("├─────────────────────────────────────────────────────────────┤");

    if let Some(config) = probe.alarm_config() {
        let any_enabled = config.any_enabled();
        let any_tripped = config.any_tripped();
        let any_alarming = config.any_alarming();

        if any_alarming {
            println!("│  STATUS: 🔔 ALARMING                                        │");
        } else if any_tripped {
            println!("│  STATUS: ⚠️  TRIPPED                                         │");
        } else if any_enabled {
            println!("│  STATUS: ✓ Armed                                            │");
        } else {
            println!("│  STATUS: No alarms enabled                                  │");
        }
        println!("│                                                             │");

        // Show core alarms
        let core_high = config.core_high_alarm();
        let core_low = config.core_low_alarm();

        if core_high.is_enabled() {
            let status = if core_high.is_alarming() {
                "🔔"
            } else if core_high.is_tripped() {
                "⚠️ "
            } else {
                "  "
            };
            println!(
                "│  Core High: {:6.1}°C ({:6.1}°F) {}                         │",
                core_high.temperature,
                celsius_to_fahrenheit(core_high.temperature),
                status
            );
        }

        if core_low.is_enabled() {
            let status = if core_low.is_alarming() {
                "🔔"
            } else if core_low.is_tripped() {
                "⚠️ "
            } else {
                "  "
            };
            println!(
                "│  Core Low:  {:6.1}°C ({:6.1}°F) {}                         │",
                core_low.temperature,
                celsius_to_fahrenheit(core_low.temperature),
                status
            );
        }

        // Show any other enabled alarms
        for i in 0..8 {
            if let Some(alarm) = config.high_alarm(i) {
                if alarm.is_enabled() {
                    let status = if alarm.is_alarming() {
                        "🔔"
                    } else if alarm.is_tripped() {
                        "⚠️ "
                    } else {
                        "  "
                    };
                    println!(
                        "│  T{} High:   {:6.1}°C ({:6.1}°F) {}                         │",
                        i + 1,
                        alarm.temperature,
                        celsius_to_fahrenheit(alarm.temperature),
                        status
                    );
                }
            }
            if let Some(alarm) = config.low_alarm(i) {
                if alarm.is_enabled() {
                    let status = if alarm.is_alarming() {
                        "🔔"
                    } else if alarm.is_tripped() {
                        "⚠️ "
                    } else {
                        "  "
                    };
                    println!(
                        "│  T{} Low:    {:6.1}°C ({:6.1}°F) {}                         │",
                        i + 1,
                        alarm.temperature,
                        celsius_to_fahrenheit(alarm.temperature),
                        status
                    );
                }
            }
        }
    } else {
        println!("│  Waiting for alarm data...                                 │");
    }

    println!("└─────────────────────────────────────────────────────────────┘\n");

    println!("Press Ctrl+C to return to menu");
    let _ = std::io::stdout().flush();
}
