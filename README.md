# combustion-rust-ble

A cross-platform Rust library for communicating with [Combustion Inc's](https://combustion.inc) Predictive Thermometer probes via Bluetooth Low Energy.

[![Crates.io](https://img.shields.io/crates/v/combustion-rust-ble.svg)](https://crates.io/crates/combustion-rust-ble)
[![Documentation](https://docs.rs/combustion-rust-ble/badge.svg)](https://docs.rs/combustion-rust-ble)
[![License](https://img.shields.io/crates/l/combustion-rust-ble.svg)](LICENSE-MIT)

## Features

- **Probe Discovery**: Automatically discover nearby Combustion probes
- **Real-time Temperatures**: Read all 8 sensors in real-time
- **Virtual Sensors**: Computed Core, Surface, and Ambient temperatures with dynamic sensor selection
- **Temperature Logging**: Download complete temperature history
- **Prediction Engine**: Set target temperatures and get time-to-removal predictions
- **Food Safety**: SafeCook/USDA Safe compliance monitoring
- **Temperature Alarms**: High/low temperature alarms for all sensors with audible alerts
- **Power Mode Control**: Configure auto power-off behavior
- **Multi-probe Support**: Manage up to 8 probes simultaneously
- **MeatNet Support**: Optional mesh networking between probes

## Supported Platforms

- **macOS** (Big Sur and later)
- **Windows 10+**
- **Linux** (via BlueZ)
- **iOS** (via CoreBluetooth)
- **Android** (via JNI)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
combustion-rust-ble = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use combustion_rust_ble::{DeviceManager, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Create device manager and start scanning
    let manager = DeviceManager::new().await?;
    manager.start_scanning().await?;

    // Wait for probes to be discovered
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Get all discovered probes
    for (id, probe) in manager.probes() {
        println!("Found probe: {} ({})", probe.serial_number_string(), id);

        // Read current temperatures
        let temps = probe.current_temperatures();
        let virtual_temps = probe.virtual_temperatures();

        if let Some(core) = virtual_temps.core {
            println!("  Core temperature: {:.1}°C", core);
        }
    }

    manager.shutdown().await?;
    Ok(())
}
```

## Examples

Run the examples with `cargo run --example <name>`:

| Example | Description |
|---------|-------------|
| `discover_probes` | Basic probe discovery - find all nearby probes |
| `temperature_monitor` | Real-time temperature display with all 8 sensors |
| `prediction_cooking` | Set target temperature and monitor countdown timer |
| `log_download` | Download complete temperature history from probe |
| `multi_probe` | Manage multiple probes simultaneously |
| `food_safety` | SafeCook feature demonstration |
| `alarm_control` | Temperature alarm and power mode control |
| `probe_dashboard` | Full-featured TUI dashboard with ratatui |
| `probe_debug` | Debug tool for BLE communication and data parsing |

```bash
# Discover all nearby probes
cargo run --example discover_probes

# Monitor temperatures in real-time
cargo run --example temperature_monitor

# Interactive TUI dashboard
cargo run --example probe_dashboard

# Debug BLE communication (with trace logging)
cargo run --example probe_debug
```

### probe_dashboard

A comprehensive terminal UI dashboard featuring:
- Real-time temperature visualization for all 8 sensors
- Virtual sensor display (Core, Surface, Ambient)
- Prediction configuration and monitoring
- Food safety status
- Log synchronization progress
- Multi-probe support with probe selection

**Keyboard Controls:**
| Key | Action |
|-----|--------|
| `↑/↓` | Navigate probe list |
| `Enter` | Connect/disconnect selected probe |
| `P` | Set prediction target |
| `C` | Cancel prediction |
| `F` | Configure food safety |
| `X` | Reset food safety |
| `I` | Set probe ID (1-8) |
| `O` | Cycle probe color |
| `E` | Export logs to CSV |
| `S` | Start/stop scanning |
| `U` | Toggle temperature units (°C/°F) |
| `?` | Show help |
| `Q/Esc` | Quit |

## Platform Notes

### macOS

Requires Bluetooth permission. For bundled applications, add to your `Info.plist`:

```xml
<key>NSBluetoothAlwaysUsageDescription</key>
<string>This app uses Bluetooth to communicate with Combustion thermometer probes.</string>
```

### Linux

Requires BlueZ 5.x or later. Your user may need to be in the `bluetooth` group:

```bash
sudo usermod -a -G bluetooth $USER
```

### Windows

Requires Windows 10 or later with Bluetooth LE support.

## Feature Flags

- `serde`: Enable serialization/deserialization for data types

```toml
[dependencies]
combustion-rust-ble = { version = "0.1", features = ["serde"] }
```

## API Reference

### DeviceManager

Central manager for probe discovery and management.

```rust
use combustion_rust_ble::{DeviceManager, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = DeviceManager::new().await?;

    // Scanning
    manager.start_scanning().await?;
    manager.stop_scanning().await?;
    manager.is_scanning();

    // Probe access
    manager.probes();                    // Get all discovered probes
    manager.get_probe("serial");         // Get probe by serial number
    manager.get_nearest_probe();         // Get probe with strongest signal
    manager.get_probes_by_signal();      // Get all probes sorted by signal strength
    manager.probe_count();               // Number of discovered probes

    // Callbacks
    manager.on_probe_discovered(|probe| {
        println!("Found: {}", probe.serial_number_string());
    });
    manager.on_probe_stale(|probe| {
        println!("Lost: {}", probe.serial_number_string());
    });

    // MeatNet (mesh networking)
    manager.enable_meatnet();
    manager.disable_meatnet();
    manager.is_meatnet_enabled();

    // Cleanup
    manager.shutdown().await?;

    Ok(())
}
```

### Probe

Represents a single thermometer probe.

#### Identity & Status

```rust
probe.serial_number();           // u32 serial number
probe.serial_number_string();    // Formatted string (e.g., "ABC123")
probe.identifier();              // BLE identifier
probe.id();                      // Probe ID (1-8)
probe.color();                   // Probe color
probe.mode();                    // ProbeMode (Normal, InstantRead, Reserved, Error)
probe.battery_status();          // BatteryStatus (Ok, Low)
probe.rssi();                    // Signal strength in dBm
probe.is_stale();                // True if no recent advertising data
```

#### Connection

```rust
probe.connect().await?;
probe.disconnect().await?;
probe.connection_state();        // ConnectionState (Disconnected, Connecting, Connected, Disconnecting)
probe.is_maintaining_connection();
```

#### Temperatures

```rust
// Raw temperatures (all 8 sensors)
let temps = probe.current_temperatures();
for (i, celsius) in temps.to_celsius().iter().enumerate() {
    if let Some(t) = celsius {
        println!("T{}: {:.1}°C", i + 1, t);
    }
}

// Virtual temperatures (Core, Surface, Ambient)
let vt = probe.virtual_temperatures();
println!("Core [{}]: {:?}", vt.sensor_selection.core_sensor_name(), vt.core);
println!("Surface [{}]: {:?}", vt.sensor_selection.surface_sensor_name(), vt.surface);
println!("Ambient [{}]: {:?}", vt.sensor_selection.ambient_sensor_name(), vt.ambient);

// Overheating detection
let overheating = probe.overheating();
if overheating.is_any_overheating() {
    for idx in overheating.overheating_indices() {
        println!("T{} is overheating!", idx + 1);
    }
}

// Subscribe to temperature updates
probe.on_temperatures_updated(|update| {
    println!("New temps: {:?}", update.temperatures);
});
```

#### Prediction

```rust
use combustion_rust_ble::PredictionMode;

// Set prediction target
probe.set_prediction(PredictionMode::TimeToRemoval, 63.0).await?;  // 63°C target
probe.set_prediction(PredictionMode::RemovalAndResting, 57.0).await?;

// Check prediction status
if let Some(info) = probe.prediction_info() {
    println!("State: {:?}", info.state);
    println!("Set point: {:.1}°C", info.set_point_temperature);
    println!("Estimated core: {:.1}°C", info.estimated_core_temperature);

    // Format prediction time as HH:MM:SS
    let secs = info.prediction_value_seconds;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let seconds = secs % 60;
    println!("Time remaining: {:02}:{:02}:{:02} ({} seconds)", hours, mins, seconds, secs);

    // Check progress
    if let Some(progress) = info.temperature_progress() {
        println!("Progress: {:.1}%", progress);
    }
}

// Cancel prediction
probe.cancel_prediction().await?;

// Subscribe to prediction updates
probe.on_prediction_updated(|info| {
    println!("Prediction update: {:?}", info.state);
});
```

#### Food Safety

```rust
use combustion_rust_ble::{FoodSafeProduct, FoodSafeServingState};

// Configure food safety monitoring
probe.configure_food_safe(FoodSafeProduct::ChickenBreast).await?;
probe.configure_food_safe(FoodSafeProduct::GroundBeef).await?;
probe.configure_food_safe(FoodSafeProduct::BeefSteak).await?;
probe.configure_food_safe(FoodSafeProduct::PorkChop).await?;
probe.configure_food_safe(FoodSafeProduct::Salmon).await?;

// Check food safety status
if let Some(data) = probe.food_safe_data() {
    println!("Product: {:?}", data.product);
    println!("Safe to serve: {}", data.is_safe());
    println!("Log reduction: {:.2}", data.log_reduction);
    println!("Progress: {:.1}%", data.progress_percent());
    println!("Seconds above threshold: {}", data.seconds_above_threshold);
}

// Reset food safety
probe.reset_food_safe().await?;
```

#### Temperature Logging

```rust
// Get log sync status
println!("Min sequence: {}", probe.min_sequence_number());
println!("Max sequence: {}", probe.max_sequence_number());
println!("Sync progress: {:.1}%", probe.percent_of_logs_synced());

// Access temperature log
let log = probe.temperature_log();
println!("Log entries: {}", log.len());

// Export to CSV
let csv = log.to_csv();
std::fs::write("temperature_log.csv", csv)?;

// Subscribe to log sync progress
probe.on_log_sync_progress(|percent| {
    println!("Log sync: {:.1}%", percent);
});
```

#### Temperature Alarms

```rust
use combustion_rust_ble::AlarmConfig;

// Set a high temperature alarm for the core (virtual) sensor
probe.set_core_high_alarm(74.0).await?;  // 74°C (165°F) - poultry safe temp

// Set a low temperature alarm for the core sensor
probe.set_core_low_alarm(4.0).await?;  // 4°C (40°F) - refrigeration temp

// Configure multiple alarms with full control
let mut config = AlarmConfig::new();
config.set_core_high_alarm(63.0, true);      // Beef/pork safe temp
config.set_surface_high_alarm(200.0, true);  // Grill surface alert
config.set_ambient_low_alarm(0.0, true);     // Freezing warning
probe.set_alarms(&config).await?;

// Check alarm status
if probe.any_alarm_alarming() {
    println!("Alarm is sounding!");
    probe.silence_alarms().await?;
}

if probe.any_alarm_tripped() {
    println!("An alarm threshold was crossed");
}

// Get detailed alarm configuration
if let Some(config) = probe.alarm_config() {
    let core_high = config.core_high_alarm();
    if core_high.is_enabled() {
        println!("Core high alarm set to: {:.1}°C", core_high.temperature);
    }
}

// Disable all alarms
probe.disable_all_alarms().await?;
```

#### Power Mode

```rust
use combustion_rust_ble::PowerMode;

// Check current power mode
if let Some(mode) = probe.power_mode() {
    println!("Power mode: {}", mode.name());
}

// Set power mode to Always On (probe stays powered in charger)
probe.set_power_mode(PowerMode::AlwaysOn).await?;

// Set power mode to Normal (auto power-off in charger)
probe.set_power_mode(PowerMode::Normal).await?;

// Check if probe is in always-on mode
if probe.is_always_on() {
    println!("Probe will stay powered in charger");
}

// Reset thermometer to factory defaults
// WARNING: Resets probe ID, color, alarms, etc.
probe.reset_thermometer().await?;
```

#### Probe Configuration

```rust
use combustion_rust_ble::{ProbeId, ProbeColor};

// Set probe ID (1-8)
probe.set_id(ProbeId::ID1).await?;

// Set probe color
probe.set_color(ProbeColor::Blue).await?;

// Read device info
let firmware = probe.read_firmware_version().await?;
let hardware = probe.read_hardware_revision().await?;
let session = probe.read_session_info().await?;
println!("Firmware: {}", firmware);
println!("Hardware: {}", hardware);
println!("Session ID: {}", session.session_id);
println!("Sample period: {}ms", session.sample_period_ms());
```

### Temperature Types

```rust
use combustion_rust_ble::{RawTemperature, ProbeTemperatures, VirtualTemperatures};

// Raw 13-bit temperature value
let raw = RawTemperature::new(2260);
let celsius = raw.to_celsius();      // Some(63.0)
let fahrenheit = raw.to_fahrenheit(); // Some(145.4)

// Create from Celsius/Fahrenheit
let raw = RawTemperature::from_celsius(63.0);
let raw = RawTemperature::from_fahrenheit(145.4);

// Check validity
if raw.is_valid() {
    println!("Temperature: {:.1}°C", raw.to_celsius().unwrap());
}
```

### Utility Functions

```rust
use combustion_rust_ble::{celsius_to_fahrenheit, fahrenheit_to_celsius};

let fahrenheit = celsius_to_fahrenheit(100.0);  // 212.0
let celsius = fahrenheit_to_celsius(212.0);     // 100.0
```

### Enums

```rust
use combustion_rust_ble::{
    ProbeMode,           // Normal, InstantRead, Reserved, Error
    ProbeId,             // ID1-ID8
    ProbeColor,          // Yellow, Grey, Red, Orange, Blue, Green, Purple, Pink
    BatteryStatus,       // Ok, Low
    ConnectionState,     // Disconnected, Connecting, Connected, Disconnecting
    PredictionMode,      // None, TimeToRemoval, RemovalAndResting, Reserved
    PredictionState,     // ProbeNotInserted, ProbeInserted, Warming, Predicting, RemovalPredictionDone, ...
    PredictionType,      // None, Removal, Resting, Reserved
    FoodSafeProduct,     // ChickenBreast, GroundBeef, BeefSteak, PorkChop, Salmon, Custom(...)
    FoodSafeServingState, // SafeToServe, NotSafe
    PowerMode,           // Normal, AlwaysOn
};
```

### Structs

```rust
use combustion_rust_ble::{
    AlarmConfig,              // High/low alarm configuration for all sensors
    AlarmStatus,              // Individual alarm status (set, tripped, alarming, temperature)
    ThermometerPreferences,   // Power mode and other thermometer settings
};
```

## Documentation

Full API documentation is available at [docs.rs](https://docs.rs/combustion-rust-ble).

## Related Projects

- [combustion-ios-ble](https://github.com/combustion-inc/combustion-ios-ble) - Official iOS SDK
- [combustion-android-ble](https://github.com/combustion-inc/combustion-android-ble) - Official Android SDK
- [combustion_ble](https://github.com/legrego/combustion_ble) - Python SDK

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
