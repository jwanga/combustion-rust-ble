//! Food safety (SafeCook) monitoring example
//!
//! Run with: cargo run --example food_safety

use combustion_rust_ble::{
    celsius_to_fahrenheit, DeviceManager, Error, FoodSafeProduct, FoodSafeServingState, Result,
};
use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("warn").init();

    println!("Food Safety Monitor (SafeCook)");
    println!("==============================\n");

    println!("Available food products:");
    println!("  1. Chicken Breast (7.0 log reduction @ 74°C)");
    println!("  2. Ground Beef (6.5 log reduction @ 70°C)");
    println!("  3. Beef Steak (6.5 log reduction @ 70°C)");
    println!("  4. Pork Chop (6.5 log reduction @ 70°C)");
    println!("  5. Salmon (6.0 log reduction @ 63°C)");
    println!();

    // Default to chicken breast for demo
    let product = FoodSafeProduct::ChickenBreast;
    println!(
        "Using: {:?} (requires {:.1} log reduction at {:.1}°C reference)",
        product,
        product.default_log_reduction(),
        product.reference_temperature()
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

    // Configure food safety
    println!("Configuring food safety monitoring...");
    probe.configure_food_safe(product).await?;

    println!("Food safety monitoring active!\n");
    println!("Insert probe into food and begin cooking.");
    println!("Press Ctrl+C to exit.\n");

    let start_time = std::time::Instant::now();
    let mut safe_notified = false;

    // Monitor loop
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nExiting...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                display_food_safety_status(&probe, &product, start_time.elapsed(), &mut safe_notified);
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
    product: &FoodSafeProduct,
    elapsed: Duration,
    safe_notified: &mut bool,
) {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");

    println!("=== Food Safety Monitor ===\n");
    println!("Probe: {}", probe.serial_number_string());
    println!("Product: {:?}", product);
    println!(
        "Elapsed: {:02}:{:02}",
        elapsed.as_secs() / 60,
        elapsed.as_secs() % 60
    );
    println!();

    let vt = probe.virtual_temperatures();
    let sel = &vt.sensor_selection;

    // Temperature display
    println!("Current Temperatures:");
    println!("---------------------");
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
    }

    // Food safety data
    println!("\nFood Safety Status:");
    println!("-------------------");

    if let Some(data) = probe.food_safe_data() {
        let target_reduction = product.default_log_reduction();
        let progress = data.progress_percent();

        // Progress bar
        let bar_width = 30;
        let filled = ((progress / 100.0) * bar_width as f64) as usize;
        let empty = bar_width - filled;
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

        println!(
            "  Log Reduction: {:.2} / {:.1}",
            data.log_reduction, target_reduction
        );
        println!("  Progress:      {} {:.1}%", bar, progress);
        println!("  Time at Temp:  {} seconds", data.seconds_above_threshold);

        match data.serving_state {
            FoodSafeServingState::SafeToServe => {
                println!("\n  ✅ SAFE TO SERVE");
                if !*safe_notified {
                    println!("\n  🎉 Food has reached safe serving criteria!");
                    *safe_notified = true;
                }
            }
            FoodSafeServingState::NotSafe => {
                println!("\n  ⏳ NOT YET SAFE - Continue cooking");

                // Calculate rough estimate based on current temperature
                if let Some(core) = vt.core {
                    let ref_temp = product.reference_temperature();
                    if core < ref_temp {
                        println!(
                            "\n  Target: {:.1}°C minimum ({:.1}°F)",
                            ref_temp,
                            celsius_to_fahrenheit(ref_temp)
                        );
                        println!("  Need:   {:.1}°C more", ref_temp - core);
                    }
                }
            }
        }
    } else {
        println!("  Waiting for food safety data...");
    }

    // Safety guidelines
    println!("\n{}", "-".repeat(40));
    println!("USDA Safe Temperature Guidelines:");
    println!("  Poultry: 74°C (165°F) instant");
    println!("  Ground Meat: 71°C (160°F)");
    println!("  Whole Cuts: 63°C (145°F) + 3 min rest");
    println!("  Fish: 63°C (145°F)");

    println!("\nPress Ctrl+C to exit");
    let _ = std::io::stdout().flush();
}
