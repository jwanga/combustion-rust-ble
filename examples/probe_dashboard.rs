//! Comprehensive TUI dashboard for Combustion probe monitoring and debugging
//!
//! Run with: cargo run --example probe_dashboard
//!
//! This example provides a full-featured terminal interface for:
//! - Discovering and monitoring multiple probes simultaneously
//! - Real-time temperature visualization
//! - Prediction and food safety configuration
//! - Temperature alarm configuration and monitoring
//! - Power mode control
//! - Log synchronization and export
//! - Debugging connectivity and protocol issues
//!
//! ## Keyboard Controls
//!
//! | Key | Action |
//! |-----|--------|
//! | `Up/Down` | Navigate probe list |
//! | `Enter` | Connect/disconnect selected probe |
//! | `P` | Set prediction target |
//! | `C` | Cancel prediction |
//! | `F` | Configure food safety |
//! | `X` | Reset food safety |
//! | `A` | Configure temperature alarms |
//! | `M` | Silence alarms |
//! | `W` | Toggle power mode |
//! | `R` | Reset thermometer |
//! | `I` | Set probe ID |
//! | `O` | Cycle probe color |
//! | `L` | Download logs |
//! | `E` | Export logs to CSV |
//! | `S` | Start/stop scanning |
//! | `U` | Toggle temperature units |
//! | `?` | Show help |
//! | `Q/Esc` | Quit |

use combustion_rust_ble::{
    celsius_to_fahrenheit, BatteryStatus, ConnectionState, DeviceManager, FoodSafeConfig,
    FoodSafeMode, FoodSafeServingState, FoodSafeState, IntegratedProduct, PowerMode,
    PredictionMode, PredictionState, PredictionType, Probe, ProbeColor, ProbeMode, Result,
    Serving, SimplifiedProduct,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{block::Title, *},
};
use std::io::{self, stdout, Stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Temperature unit preference
#[derive(Clone, Copy, PartialEq, Eq)]
enum TemperatureUnit {
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    fn format(&self, celsius: f64) -> String {
        match self {
            TemperatureUnit::Celsius => format!("{:.1}°C", celsius),
            TemperatureUnit::Fahrenheit => format!("{:.1}°F", celsius_to_fahrenheit(celsius)),
        }
    }

    fn format_dual(&self, celsius: f64) -> String {
        match self {
            TemperatureUnit::Celsius => {
                format!("{:.1}°C ({:.1}°F)", celsius, celsius_to_fahrenheit(celsius))
            }
            TemperatureUnit::Fahrenheit => {
                format!("{:.1}°F ({:.1}°C)", celsius_to_fahrenheit(celsius), celsius)
            }
        }
    }
}

/// Log severity level
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn style(&self) -> Style {
        match self {
            LogLevel::Debug => Style::default().fg(Color::DarkGray),
            LogLevel::Info => Style::default().fg(Color::Cyan),
            LogLevel::Warn => Style::default().fg(Color::Yellow),
            LogLevel::Error => Style::default().fg(Color::Red),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO ",
            LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }
}

/// Event log entry
struct LogEntry {
    timestamp: Instant,
    level: LogLevel,
    message: String,
}

/// Input dialog type
#[derive(Clone)]
#[allow(dead_code)]
enum DialogType {
    SetPrediction,
    SetFoodSafe,
    SetProbeId,
    SetProbeColor,
    SetAlarm,
    ConfirmReset,
    Help,
}

/// Alarm dialog sub-state
#[derive(Clone, Default)]
struct AlarmDialogState {
    /// 0 = Core High, 1 = Core Low, 2 = Surface High, 3 = Ambient Low, 4 = Disable All
    selected_alarm_type: usize,
    /// Temperature input for alarm threshold
    temp_input: String,
    /// Current stage: 0 = select type, 1 = enter temp (if applicable)
    stage: usize,
}

/// Food Safe dialog sub-state
#[derive(Clone, Default)]
struct FoodSafeDialogState {
    /// 0 = Simplified, 1 = Integrated
    selected_mode: usize,
    /// Selected product index (depends on mode)
    selected_product: usize,
    /// 0 = Served Immediately, 1 = Cooked and Chilled
    selected_serving: usize,
    /// Current input stage: 0 = mode, 1 = product, 2 = serving
    stage: usize,
}

/// Dialog state for input
struct DialogState {
    dialog_type: DialogType,
    input: String,
    selected_option: usize,
    /// For SetPrediction: 0 = entering temp, 1 = selecting mode
    input_stage: usize,
    /// Selected prediction mode (0 = TimeToRemoval, 1 = RemovalAndResting)
    selected_mode: usize,
    /// Food safe dialog state
    food_safe: FoodSafeDialogState,
    /// Alarm dialog state
    alarm: AlarmDialogState,
}

/// Main application state
struct App {
    device_manager: DeviceManager,
    probes: Vec<Arc<Probe>>,
    selected_probe_index: usize,
    temperature_unit: TemperatureUnit,
    event_log: Vec<LogEntry>,
    max_log_entries: usize,
    is_scanning: bool,
    show_help: bool,
    dialog: Option<DialogState>,
    start_time: Instant,
}

impl App {
    async fn new() -> Result<Self> {
        let device_manager = DeviceManager::new().await?;

        Ok(Self {
            device_manager,
            probes: Vec::new(),
            selected_probe_index: 0,
            temperature_unit: TemperatureUnit::Celsius,
            event_log: Vec::new(),
            max_log_entries: 100,
            is_scanning: false,
            show_help: false,
            dialog: None,
            start_time: Instant::now(),
        })
    }

    fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.event_log.push(LogEntry {
            timestamp: Instant::now(),
            level,
            message: message.into(),
        });

        // Trim old entries
        if self.event_log.len() > self.max_log_entries {
            self.event_log.remove(0);
        }
    }

    fn selected_probe(&self) -> Option<&Arc<Probe>> {
        self.probes.get(self.selected_probe_index)
    }

    async fn start_scanning(&mut self) -> Result<()> {
        self.device_manager.start_scanning().await?;
        self.is_scanning = true;
        self.log(LogLevel::Info, "Started BLE scanning");
        Ok(())
    }

    async fn stop_scanning(&mut self) -> Result<()> {
        self.device_manager.stop_scanning().await?;
        self.is_scanning = false;
        self.log(LogLevel::Info, "Stopped BLE scanning");
        Ok(())
    }

    async fn toggle_scanning(&mut self) -> Result<()> {
        if self.is_scanning {
            self.stop_scanning().await
        } else {
            self.start_scanning().await
        }
    }

    fn update_probes(&mut self) {
        let new_probes: Vec<Arc<Probe>> = self.device_manager.probes().values().cloned().collect();

        // Log new discoveries
        for probe in &new_probes {
            if !self.probes.iter().any(|p| p.id() == probe.id()) {
                self.log(
                    LogLevel::Info,
                    format!(
                        "Discovered probe: {} (ID: {})",
                        probe.serial_number_string(),
                        probe.id()
                    ),
                );
            }
        }

        self.probes = new_probes;

        // Ensure selected index is valid
        if self.selected_probe_index >= self.probes.len() && !self.probes.is_empty() {
            self.selected_probe_index = self.probes.len() - 1;
        }
    }

    fn select_next_probe(&mut self) {
        if !self.probes.is_empty() {
            self.selected_probe_index = (self.selected_probe_index + 1) % self.probes.len();
        }
    }

    fn select_prev_probe(&mut self) {
        if !self.probes.is_empty() {
            self.selected_probe_index = if self.selected_probe_index == 0 {
                self.probes.len() - 1
            } else {
                self.selected_probe_index - 1
            };
        }
    }

    async fn toggle_connection(&mut self) -> Result<()> {
        if let Some(probe) = self.selected_probe().cloned() {
            match probe.connection_state() {
                ConnectionState::Connected => {
                    self.log(
                        LogLevel::Info,
                        format!("Disconnecting from {}", probe.serial_number_string()),
                    );
                    probe.disconnect().await?;
                }
                ConnectionState::Disconnected => {
                    self.log(
                        LogLevel::Info,
                        format!("Connecting to {}", probe.serial_number_string()),
                    );
                    probe.connect().await?;
                    self.log(
                        LogLevel::Info,
                        format!("Connected to {}", probe.serial_number_string()),
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn toggle_unit(&mut self) {
        self.temperature_unit = match self.temperature_unit {
            TemperatureUnit::Celsius => TemperatureUnit::Fahrenheit,
            TemperatureUnit::Fahrenheit => TemperatureUnit::Celsius,
        };
        self.log(
            LogLevel::Info,
            format!(
                "Switched to {}",
                match self.temperature_unit {
                    TemperatureUnit::Celsius => "Celsius",
                    TemperatureUnit::Fahrenheit => "Fahrenheit",
                }
            ),
        );
    }

    fn open_dialog(&mut self, dialog_type: DialogType) {
        self.dialog = Some(DialogState {
            dialog_type,
            input: String::new(),
            selected_option: 0,
            input_stage: 0,
            selected_mode: 0,
            food_safe: FoodSafeDialogState::default(),
            alarm: AlarmDialogState::default(),
        });
    }

    fn close_dialog(&mut self) {
        self.dialog = None;
    }

    async fn handle_dialog_confirm(&mut self) -> Result<()> {
        if let Some(dialog) = self.dialog.take() {
            match dialog.dialog_type {
                DialogType::SetPrediction => {
                    if let Ok(temp) = dialog.input.parse::<f64>() {
                        if let Some(probe) = self.selected_probe().cloned() {
                            let mode = match dialog.selected_mode {
                                0 => PredictionMode::TimeToRemoval,
                                1 => PredictionMode::RemovalAndResting,
                                _ => PredictionMode::TimeToRemoval,
                            };
                            probe.set_prediction(mode, temp).await?;
                            self.log(
                                LogLevel::Info,
                                format!("Set prediction: {:.1}°C, mode: {:?}", temp, mode),
                            );
                        }
                    } else {
                        self.log(LogLevel::Error, "Invalid temperature value");
                    }
                }
                DialogType::SetFoodSafe => {
                    let fs = &dialog.food_safe;
                    let serving = match fs.selected_serving {
                        0 => Serving::ServedImmediately,
                        _ => Serving::CookedAndChilled,
                    };

                    let config = if fs.selected_mode == 0 {
                        // Simplified mode
                        let simplified_products = get_simplified_products();
                        if let Some((product, _)) = simplified_products.get(fs.selected_product) {
                            FoodSafeConfig::simplified(*product, serving)
                        } else {
                            self.log(LogLevel::Error, "Invalid product selection");
                            return Ok(());
                        }
                    } else {
                        // Integrated mode
                        let integrated_products = get_integrated_products();
                        if let Some((product, _)) = integrated_products.get(fs.selected_product) {
                            FoodSafeConfig::integrated(*product, serving)
                        } else {
                            self.log(LogLevel::Error, "Invalid product selection");
                            return Ok(());
                        }
                    };

                    if let Some(probe) = self.selected_probe().cloned() {
                        probe.configure_food_safe_with_config(config.clone()).await?;
                        let mode_str = if fs.selected_mode == 0 {
                            "Simplified"
                        } else {
                            "Integrated"
                        };
                        let serving_str = match serving {
                            Serving::ServedImmediately => "Served Immediately",
                            Serving::CookedAndChilled => "Cooked and Chilled",
                        };
                        self.log(
                            LogLevel::Info,
                            format!(
                                "Configured food safety: {} mode, product #{}, {}",
                                mode_str,
                                fs.selected_product,
                                serving_str
                            ),
                        );
                    }
                }
                DialogType::SetProbeId => {
                    if let Ok(id) = dialog.input.parse::<u8>() {
                        if (1..=8).contains(&id) {
                            if let Some(probe) = self.selected_probe().cloned() {
                                probe
                                    .set_id(combustion_rust_ble::ProbeId::from_raw(id - 1))
                                    .await?;
                                self.log(LogLevel::Info, format!("Set probe ID to {}", id));
                            }
                        } else {
                            self.log(LogLevel::Error, "Probe ID must be 1-8");
                        }
                    }
                }
                DialogType::SetProbeColor => {
                    // Parse color from selected option
                    let colors = [
                        ProbeColor::Yellow,
                        ProbeColor::Grey,
                        ProbeColor::Red,
                        ProbeColor::Orange,
                        ProbeColor::Blue,
                        ProbeColor::Green,
                        ProbeColor::Purple,
                        ProbeColor::Pink,
                    ];
                    if dialog.selected_option < colors.len() {
                        if let Some(probe) = self.selected_probe().cloned() {
                            let color = colors[dialog.selected_option];
                            probe.set_color(color).await?;
                            self.log(LogLevel::Info, format!("Set probe color to {:?}", color));
                        }
                    }
                }
                DialogType::SetAlarm => {
                    let alarm = &dialog.alarm;
                    if let Some(probe) = self.selected_probe().cloned() {
                        match alarm.selected_alarm_type {
                            0 => {
                                // Core High
                                if let Ok(temp) = alarm.temp_input.parse::<f64>() {
                                    probe.set_core_high_alarm(temp).await?;
                                    self.log(
                                        LogLevel::Info,
                                        format!("Set core HIGH alarm to {:.1}°C", temp),
                                    );
                                }
                            }
                            1 => {
                                // Core Low
                                if let Ok(temp) = alarm.temp_input.parse::<f64>() {
                                    probe.set_core_low_alarm(temp).await?;
                                    self.log(
                                        LogLevel::Info,
                                        format!("Set core LOW alarm to {:.1}°C", temp),
                                    );
                                }
                            }
                            2 => {
                                // Surface High
                                if let Ok(temp) = alarm.temp_input.parse::<f64>() {
                                    let mut config = probe.alarm_config().unwrap_or_default();
                                    config.set_surface_high_alarm(temp, true);
                                    probe.set_alarms(&config).await?;
                                    self.log(
                                        LogLevel::Info,
                                        format!("Set surface HIGH alarm to {:.1}°C", temp),
                                    );
                                }
                            }
                            3 => {
                                // Ambient Low
                                if let Ok(temp) = alarm.temp_input.parse::<f64>() {
                                    let mut config = probe.alarm_config().unwrap_or_default();
                                    config.set_ambient_low_alarm(temp, true);
                                    probe.set_alarms(&config).await?;
                                    self.log(
                                        LogLevel::Info,
                                        format!("Set ambient LOW alarm to {:.1}°C", temp),
                                    );
                                }
                            }
                            4 => {
                                // Disable All
                                probe.disable_all_alarms().await?;
                                self.log(LogLevel::Info, "Disabled all alarms");
                            }
                            _ => {}
                        }
                    }
                }
                DialogType::ConfirmReset => {
                    if let Some(probe) = self.selected_probe().cloned() {
                        probe.reset_thermometer().await?;
                        self.log(LogLevel::Warn, "Thermometer reset to factory defaults");
                    }
                }
                DialogType::Help => {}
            }
        }
        Ok(())
    }

    async fn cancel_prediction(&mut self) -> Result<()> {
        if let Some(probe) = self.selected_probe().cloned() {
            probe.cancel_prediction().await?;
            self.log(LogLevel::Info, "Cancelled prediction");
        }
        Ok(())
    }

    async fn reset_food_safe(&mut self) -> Result<()> {
        if let Some(probe) = self.selected_probe().cloned() {
            probe.reset_food_safe().await?;
            self.log(LogLevel::Info, "Reset food safety");
        }
        Ok(())
    }

    async fn silence_alarms(&mut self) -> Result<()> {
        if let Some(probe) = self.selected_probe().cloned() {
            probe.silence_alarms().await?;
            self.log(LogLevel::Info, "Silenced alarms");
        }
        Ok(())
    }

    async fn toggle_power_mode(&mut self) -> Result<()> {
        if let Some(probe) = self.selected_probe().cloned() {
            let current = probe.power_mode().unwrap_or(PowerMode::Normal);
            let new_mode = match current {
                PowerMode::Normal => PowerMode::AlwaysOn,
                PowerMode::AlwaysOn => PowerMode::Normal,
            };
            probe.set_power_mode(new_mode).await?;
            self.log(LogLevel::Info, format!("Power mode set to {}", new_mode.name()));
        }
        Ok(())
    }

    fn export_logs(&mut self) {
        if let Some(probe) = self.selected_probe() {
            let log = probe.temperature_log();
            let csv = log.to_csv();
            let filename = format!("probe_{}_log.csv", probe.serial_number_string());

            match std::fs::write(&filename, &csv) {
                Ok(_) => self.log(LogLevel::Info, format!("Exported logs to {}", filename)),
                Err(e) => self.log(LogLevel::Error, format!("Failed to export: {}", e)),
            }
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Disconnect all probes
        for probe in &self.probes {
            let _ = probe.disconnect().await;
        }
        self.device_manager.shutdown().await
    }
}

/// Main terminal type alias
type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> io::Result<Terminal> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()
}

/// Get list of simplified mode products with display names
fn get_simplified_products() -> Vec<(SimplifiedProduct, &'static str)> {
    vec![
        (SimplifiedProduct::AnyPoultry, "Any Poultry (165°F/74°C)"),
        (SimplifiedProduct::BeefCuts, "Beef Cuts (145°F/63°C)"),
        (SimplifiedProduct::PorkCuts, "Pork Cuts (145°F/63°C)"),
        (SimplifiedProduct::VealCuts, "Veal Cuts (145°F/63°C)"),
        (SimplifiedProduct::LambCuts, "Lamb Cuts (145°F/63°C)"),
        (SimplifiedProduct::GroundMeats, "Ground Meats (160°F/71°C)"),
        (
            SimplifiedProduct::HamFreshOrSmoked,
            "Ham Fresh/Smoked (145°F/63°C)",
        ),
        (
            SimplifiedProduct::HamCookedAndReheated,
            "Ham Reheated (165°F/74°C)",
        ),
        (SimplifiedProduct::Eggs, "Eggs (160°F/71°C)"),
        (
            SimplifiedProduct::FishAndShellfish,
            "Fish & Shellfish (145°F/63°C)",
        ),
        (SimplifiedProduct::Leftovers, "Leftovers (165°F/74°C)"),
        (SimplifiedProduct::Casseroles, "Casseroles (165°F/74°C)"),
    ]
}

/// Get list of integrated mode products with display names
fn get_integrated_products() -> Vec<(IntegratedProduct, &'static str)> {
    vec![
        (IntegratedProduct::Poultry, "Poultry (7.0 log reduction)"),
        (IntegratedProduct::Meats, "Meats - whole muscle (5.0 log)"),
        (
            IntegratedProduct::MeatsGroundChoppedOrStuffed,
            "Meats - ground/stuffed (6.5 log)",
        ),
        (
            IntegratedProduct::PoultryGroundChoppedOrStuffed,
            "Poultry - ground/stuffed (7.0 log)",
        ),
        (IntegratedProduct::Seafood, "Seafood - whole (6.0 log)"),
        (
            IntegratedProduct::SeafoodGroundOrChopped,
            "Seafood - ground (6.0 log)",
        ),
        (IntegratedProduct::SeafoodStuffed, "Seafood - stuffed (6.0 log)"),
        (IntegratedProduct::Eggs, "Eggs (5.0 log)"),
        (IntegratedProduct::EggsYolk, "Egg Yolk (5.0 log)"),
        (IntegratedProduct::EggsWhite, "Egg White (5.0 log)"),
        (IntegratedProduct::DairyMilk, "Dairy - Milk (5.0 log)"),
        (IntegratedProduct::DairyCreams, "Dairy - Creams (5.0 log)"),
        (
            IntegratedProduct::DairyIceCreamMixEggnog,
            "Dairy - Ice Cream/Eggnog (5.0 log)",
        ),
        (IntegratedProduct::Other, "Other (6.5 log)"),
    ]
}

/// Get product display name from product code and mode
fn get_product_name(product_code: u16, mode: FoodSafeMode) -> String {
    match mode {
        FoodSafeMode::Simplified => {
            if let Some(product) = SimplifiedProduct::from_raw(product_code) {
                match product {
                    SimplifiedProduct::Default => "Default".to_string(),
                    SimplifiedProduct::AnyPoultry => "Any Poultry".to_string(),
                    SimplifiedProduct::BeefCuts => "Beef Cuts".to_string(),
                    SimplifiedProduct::PorkCuts => "Pork Cuts".to_string(),
                    SimplifiedProduct::VealCuts => "Veal Cuts".to_string(),
                    SimplifiedProduct::LambCuts => "Lamb Cuts".to_string(),
                    SimplifiedProduct::GroundMeats => "Ground Meats".to_string(),
                    SimplifiedProduct::HamFreshOrSmoked => "Ham Fresh/Smoked".to_string(),
                    SimplifiedProduct::HamCookedAndReheated => "Ham Reheated".to_string(),
                    SimplifiedProduct::Eggs => "Eggs".to_string(),
                    SimplifiedProduct::FishAndShellfish => "Fish & Shellfish".to_string(),
                    SimplifiedProduct::Leftovers => "Leftovers".to_string(),
                    SimplifiedProduct::Casseroles => "Casseroles".to_string(),
                }
            } else {
                format!("Unknown ({})", product_code)
            }
        }
        FoodSafeMode::Integrated => {
            if let Some(product) = IntegratedProduct::from_raw(product_code) {
                match product {
                    IntegratedProduct::Poultry => "Poultry".to_string(),
                    IntegratedProduct::Meats => "Meats".to_string(),
                    IntegratedProduct::MeatsGroundChoppedOrStuffed => "Ground Meats".to_string(),
                    IntegratedProduct::PoultryGroundChoppedOrStuffed => "Ground Poultry".to_string(),
                    IntegratedProduct::Seafood => "Seafood".to_string(),
                    IntegratedProduct::SeafoodGroundOrChopped => "Ground Seafood".to_string(),
                    IntegratedProduct::SeafoodStuffed => "Stuffed Seafood".to_string(),
                    IntegratedProduct::Eggs => "Eggs".to_string(),
                    IntegratedProduct::EggsYolk => "Egg Yolk".to_string(),
                    IntegratedProduct::EggsWhite => "Egg White".to_string(),
                    IntegratedProduct::DairyMilk => "Dairy - Milk".to_string(),
                    IntegratedProduct::DairyCreams => "Dairy - Creams".to_string(),
                    IntegratedProduct::DairyIceCreamMixEggnog => "Ice Cream/Eggnog".to_string(),
                    IntegratedProduct::Other => "Other".to_string(),
                    IntegratedProduct::Custom => "Custom".to_string(),
                }
            } else {
                format!("Unknown ({})", product_code)
            }
        }
    }
}

fn render_ui(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Main layout: header, content, footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(20),    // Content
            Constraint::Length(10), // Event log
            Constraint::Length(1),  // Status bar
        ])
        .split(size);

    render_header(frame, main_chunks[0], app);
    render_content(frame, main_chunks[1], app);
    render_event_log(frame, main_chunks[2], app);
    render_status_bar(frame, main_chunks[3], app);

    // Render dialog overlay if active
    if let Some(dialog) = &app.dialog {
        render_dialog(frame, dialog, size);
    }

    // Render help overlay
    if app.show_help {
        render_help_overlay(frame, size);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed = app.start_time.elapsed();
    let title = format!(
        " COMBUSTION PROBE DASHBOARD | Probes: {} | Uptime: {:02}:{:02}:{:02} ",
        app.probes.len(),
        elapsed.as_secs() / 3600,
        (elapsed.as_secs() % 3600) / 60,
        elapsed.as_secs() % 60
    );

    let header = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Title::from(title).alignment(Alignment::Center))
        .title(
            Title::from(" [?] Help  [Q] Quit ")
                .alignment(Alignment::Right)
                .position(block::Position::Top),
        );

    frame.render_widget(header, area);
}

fn render_content(frame: &mut Frame, area: Rect, app: &App) {
    // Split into left (probes list + temps) and right (details + actions)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // Left side: probe list and temperatures
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(10)])
        .split(content_chunks[0]);

    render_probe_list(frame, left_chunks[0], app);
    render_temperatures(frame, left_chunks[1], app);

    // Right side: details, actions, prediction, food safety
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),  // Probe details
            Constraint::Length(8),  // Actions
            Constraint::Length(11), // Prediction (State, Mode, Type, Setpoint, Heat Start, Est. Core, Pred. Time)
            Constraint::Min(5),     // Food safety + log sync
        ])
        .split(content_chunks[1]);

    render_probe_details(frame, right_chunks[0], app);
    render_actions(frame, right_chunks[1], app);
    render_prediction(frame, right_chunks[2], app);
    render_food_safety_and_logs(frame, right_chunks[3], app);
}

fn render_probe_list(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .probes
        .iter()
        .enumerate()
        .map(|(i, probe)| {
            let connected = probe.connection_state() == ConnectionState::Connected;
            let icon = if connected { "●" } else { "○" };
            let color_emoji = color_emoji(probe.color());
            let status = if probe.is_stale() {
                "Stale"
            } else if connected {
                "Connected"
            } else {
                "Advertising"
            };

            let style = if i == app.selected_probe_index {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if connected {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(format!("{} ", color_emoji), style),
                Span::styled(
                    format!("{} [ID:{}] ", probe.serial_number_string(), probe.id()),
                    style,
                ),
                Span::styled(status, style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Discovered Probes ")
                .title(
                    Title::from(if app.is_scanning {
                        " [Scanning] "
                    } else {
                        " [Stopped] "
                    })
                    .alignment(Alignment::Right),
                ),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    frame.render_widget(list, area);
}

fn render_temperatures(frame: &mut Frame, area: Rect, app: &App) {
    let mut rows = vec![];

    if let Some(probe) = app.selected_probe() {
        let vt = probe.virtual_temperatures();
        let sel = &vt.sensor_selection;

        // Virtual sensors header
        rows.push(Row::new(vec![
            Cell::from("─── Virtual Sensors ───").style(Style::default().fg(Color::Yellow)),
            Cell::from(""),
            Cell::from(""),
        ]));

        // Core
        if let Some(core) = vt.core {
            rows.push(Row::new(vec![
                Cell::from(format!("Core [{}]:", sel.core_sensor_name())),
                Cell::from(app.temperature_unit.format_dual(core)),
                Cell::from("✓").style(Style::default().fg(Color::Green)),
            ]));
        } else {
            rows.push(Row::new(vec![
                Cell::from(format!("Core [{}]:", sel.core_sensor_name())),
                Cell::from("--"),
                Cell::from(""),
            ]));
        }

        // Surface
        if let Some(surface) = vt.surface {
            rows.push(Row::new(vec![
                Cell::from(format!("Surface [{}]:", sel.surface_sensor_name())),
                Cell::from(app.temperature_unit.format_dual(surface)),
                Cell::from("✓").style(Style::default().fg(Color::Green)),
            ]));
        } else {
            rows.push(Row::new(vec![
                Cell::from(format!("Surface [{}]:", sel.surface_sensor_name())),
                Cell::from("--"),
                Cell::from(""),
            ]));
        }

        // Ambient
        if let Some(ambient) = vt.ambient {
            rows.push(Row::new(vec![
                Cell::from(format!("Ambient [{}]:", sel.ambient_sensor_name())),
                Cell::from(app.temperature_unit.format_dual(ambient)),
                Cell::from("✓").style(Style::default().fg(Color::Green)),
            ]));
        } else {
            rows.push(Row::new(vec![
                Cell::from(format!("Ambient [{}]:", sel.ambient_sensor_name())),
                Cell::from("--"),
                Cell::from(""),
            ]));
        }

        // Raw sensors header
        rows.push(Row::new(vec![
            Cell::from("─── Raw Sensors (T1-T8) ───").style(Style::default().fg(Color::Yellow)),
            Cell::from(""),
            Cell::from(""),
        ]));

        let temps = probe.current_temperatures();
        let celsius_temps = temps.to_celsius();
        let is_instant_read = probe.mode() == ProbeMode::InstantRead;
        let overheating = probe.overheating();

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
            let is_overheating = overheating.is_sensor_overheating(i);

            if is_instant_read && i > 0 {
                rows.push(Row::new(vec![
                    Cell::from(format!("{}:", sensor_names[i])),
                    Cell::from("N/A (Instant Read)").style(Style::default().fg(Color::DarkGray)),
                    Cell::from(""),
                ]));
            } else if let Some(c) = celsius {
                let status = if is_overheating {
                    Cell::from("⚠").style(Style::default().fg(Color::Red))
                } else {
                    Cell::from("✓").style(Style::default().fg(Color::Green))
                };

                let temp_style = if is_overheating {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };

                rows.push(Row::new(vec![
                    Cell::from(format!("{}:", sensor_names[i])),
                    Cell::from(app.temperature_unit.format_dual(*c)).style(temp_style),
                    status,
                ]));
            } else {
                rows.push(Row::new(vec![
                    Cell::from(format!("{}:", sensor_names[i])),
                    Cell::from("Invalid").style(Style::default().fg(Color::Red)),
                    Cell::from("✗").style(Style::default().fg(Color::Red)),
                ]));
            }
        }

        // Overheating summary
        if overheating.is_any_overheating() {
            rows.push(Row::new(vec![
                Cell::from("⚠ OVERHEAT WARNING")
                    .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Cell::from(""),
                Cell::from(""),
            ]));
        }
    } else {
        rows.push(Row::new(vec![
            Cell::from("No probe selected"),
            Cell::from(""),
            Cell::from(""),
        ]));
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Min(24),
            Constraint::Length(3),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Temperature Readings "),
    );

    frame.render_widget(table, area);
}

fn render_probe_details(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![];

    if let Some(probe) = app.selected_probe() {
        lines.push(Line::from(vec![
            Span::raw("Serial: "),
            Span::styled(
                probe.serial_number_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::raw("ID: "),
            Span::styled(
                format!("{}", probe.id()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" | Color: "),
            Span::styled(
                format!("{:?} {}", probe.color(), color_emoji(probe.color())),
                Style::default().fg(Color::Magenta),
            ),
        ]));

        let battery_style = match probe.battery_status() {
            BatteryStatus::Ok => Style::default().fg(Color::Green),
            BatteryStatus::Low => Style::default().fg(Color::Red),
        };
        lines.push(Line::from(vec![
            Span::raw("Battery: "),
            Span::styled(format!("{:?}", probe.battery_status()), battery_style),
        ]));

        let rssi_info = probe.rssi().map(|r| {
            let quality = if r > -50 {
                ("Excellent", Color::Green)
            } else if r > -65 {
                ("Good", Color::Yellow)
            } else if r > -80 {
                ("Fair", Color::LightYellow)
            } else {
                ("Poor", Color::Red)
            };
            (r, quality)
        });

        if let Some((rssi, (quality, color))) = rssi_info {
            lines.push(Line::from(vec![
                Span::raw("RSSI: "),
                Span::raw(format!("{} dBm", rssi)),
                Span::styled(format!(" ({})", quality), Style::default().fg(color)),
            ]));
        } else {
            lines.push(Line::from(vec![Span::raw("RSSI: "), Span::raw("N/A")]));
        }

        lines.push(Line::from(vec![
            Span::raw("Mode: "),
            Span::styled(
                format!("{:?}", probe.mode()),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        // Power mode
        let power_mode = probe.power_mode().unwrap_or(PowerMode::Normal);
        let power_style = match power_mode {
            PowerMode::Normal => Style::default().fg(Color::Green),
            PowerMode::AlwaysOn => Style::default().fg(Color::Yellow),
        };
        lines.push(Line::from(vec![
            Span::raw("Power: "),
            Span::styled(power_mode.name(), power_style),
        ]));

        let conn_style = match probe.connection_state() {
            ConnectionState::Connected => Style::default().fg(Color::Green),
            ConnectionState::Connecting | ConnectionState::Disconnecting => {
                Style::default().fg(Color::Yellow)
            }
            ConnectionState::Disconnected => Style::default().fg(Color::Red),
        };
        lines.push(Line::from(vec![
            Span::raw("Connection: "),
            Span::styled(format!("{:?}", probe.connection_state()), conn_style),
        ]));

        lines.push(Line::from(vec![
            Span::raw("Stale: "),
            if probe.is_stale() {
                Span::styled("Yes", Style::default().fg(Color::Red))
            } else {
                Span::styled("No", Style::default().fg(Color::Green))
            },
        ]));
    } else {
        lines.push(Line::from("Select a probe to view details"));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Selected Probe Details "),
    );

    frame.render_widget(paragraph, area);
}

fn render_actions(frame: &mut Frame, area: Rect, app: &App) {
    let connected = app
        .selected_probe()
        .is_some_and(|p| p.connection_state() == ConnectionState::Connected);

    let conn_hint = if !connected {
        Span::styled(" (connect first)", Style::default().fg(Color::DarkGray))
    } else {
        Span::raw("")
    };

    let actions = vec![
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" Connect  "),
            Span::styled("[P]", Style::default().fg(Color::Yellow)),
            Span::raw(" Predict  "),
            Span::styled("[C]", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel"),
            conn_hint.clone(),
        ]),
        Line::from(vec![
            Span::styled("[F]", Style::default().fg(Color::Yellow)),
            Span::raw(" Food Safe  "),
            Span::styled("[X]", Style::default().fg(Color::Yellow)),
            Span::raw(" Reset Safe  "),
            Span::styled("[I]", Style::default().fg(Color::Yellow)),
            Span::raw(" ID  "),
            Span::styled("[O]", Style::default().fg(Color::Yellow)),
            Span::raw(" Color"),
        ]),
        Line::from(vec![
            Span::styled("[A]", Style::default().fg(Color::Cyan)),
            Span::raw(" Alarms  "),
            Span::styled("[M]", Style::default().fg(Color::Cyan)),
            Span::raw(" Silence  "),
            Span::styled("[W]", Style::default().fg(Color::Cyan)),
            Span::raw(" Power  "),
            Span::styled("[R]", Style::default().fg(Color::Red)),
            Span::raw(" Reset"),
        ]),
        Line::from(vec![
            Span::styled("[E]", Style::default().fg(Color::Yellow)),
            Span::raw(" Export  "),
            Span::styled("[S]", Style::default().fg(Color::Yellow)),
            Span::raw(" Scan  "),
            Span::styled("[U]", Style::default().fg(Color::Yellow)),
            Span::raw(" Units  "),
            Span::styled("[?]", Style::default().fg(Color::Yellow)),
            Span::raw(" Help"),
        ]),
    ];

    let paragraph =
        Paragraph::new(actions).block(Block::default().borders(Borders::ALL).title(" Actions "));

    frame.render_widget(paragraph, area);
}

fn render_prediction(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![];

    if let Some(probe) = app.selected_probe() {
        if let Some(info) = probe.prediction_info() {
            // Prediction State - always shown
            let state_style = match info.state {
                PredictionState::Predicting => Style::default().fg(Color::Green),
                PredictionState::RemovalPredictionDone => Style::default().fg(Color::Cyan),
                PredictionState::Warming => Style::default().fg(Color::LightYellow),
                _ => Style::default().fg(Color::Yellow),
            };
            lines.push(Line::from(vec![
                Span::raw("State: "),
                Span::styled(format!("{:?}", info.state), state_style),
            ]));

            // Prediction Mode - always shown
            let mode_style = match info.mode {
                PredictionMode::TimeToRemoval => Style::default().fg(Color::Green),
                PredictionMode::RemovalAndResting => Style::default().fg(Color::Cyan),
                _ => Style::default().fg(Color::DarkGray),
            };
            lines.push(Line::from(vec![
                Span::raw("Mode: "),
                Span::styled(format!("{:?}", info.mode), mode_style),
            ]));

            // Prediction Type - always shown
            let type_style = match info.prediction_type {
                PredictionType::Removal => Style::default().fg(Color::Green),
                PredictionType::Resting => Style::default().fg(Color::Cyan),
                _ => Style::default().fg(Color::DarkGray),
            };
            lines.push(Line::from(vec![
                Span::raw("Type: "),
                Span::styled(format!("{:?}", info.prediction_type), type_style),
            ]));

            // Prediction Setpoint Temperature - always shown
            lines.push(Line::from(vec![
                Span::raw("Setpoint: "),
                Span::styled(
                    app.temperature_unit.format_dual(info.set_point_temperature),
                    Style::default().fg(Color::Cyan),
                ),
            ]));

            // Heat Start Temperature - always shown
            lines.push(Line::from(vec![
                Span::raw("Heat Start: "),
                Span::styled(
                    app.temperature_unit.format(info.heat_start_temperature),
                    Style::default().fg(Color::Magenta),
                ),
            ]));

            // Estimated Core Temperature - always shown
            lines.push(Line::from(vec![
                Span::raw("Est. Core: "),
                Span::styled(
                    app.temperature_unit.format(info.estimated_core_temperature),
                    Style::default().fg(Color::Yellow),
                ),
            ]));

            // Prediction Value Seconds - always shown in HH:MM:SS format
            let total_secs = info.prediction_value_seconds;
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            let secs = total_secs % 60;
            let time_style = if info.state.is_predicting() {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(vec![
                Span::raw("Pred. Time: "),
                Span::styled(
                    format!("{:02}:{:02}:{:02} ({} sec)", hours, mins, secs, total_secs),
                    time_style,
                ),
            ]));
        } else {
            // No prediction info available - show defaults
            lines.push(Line::from(vec![
                Span::raw("State: "),
                Span::styled("ProbeNotInserted", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Mode: "),
                Span::styled("None", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Type: "),
                Span::styled("None", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Setpoint: "),
                Span::styled("--", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Heat Start: "),
                Span::styled("--", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Est. Core: "),
                Span::styled("--", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Pred. Time: "),
                Span::styled("--", Style::default().fg(Color::DarkGray)),
            ]));
        }
    } else {
        lines.push(Line::from("Select a probe"));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Prediction Status "),
    );

    frame.render_widget(paragraph, area);
}

fn render_food_safety_and_logs(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // Food safety
    let mut food_lines = vec![];

    if let Some(probe) = app.selected_probe() {
        if let Some(data) = probe.food_safe_data() {
            // State with icon
            let state = data.state();
            let (state_icon, state_style) = match state {
                FoodSafeState::NotSafe => ("⏳", Style::default().fg(Color::Yellow)),
                FoodSafeState::Safe => ("✓", Style::default().fg(Color::Green)),
                FoodSafeState::SafetyImpossible => ("✗", Style::default().fg(Color::Red)),
            };

            food_lines.push(Line::from(vec![
                Span::raw("State: "),
                Span::styled(format!("{} {:?}", state_icon, state), state_style),
            ]));

            // Mode and config info
            if let Some(ref config) = data.config {
                let mode_str = match config.mode {
                    FoodSafeMode::Simplified => "Simplified",
                    FoodSafeMode::Integrated => "Integrated",
                };
                food_lines.push(Line::from(vec![
                    Span::raw("Mode: "),
                    Span::styled(mode_str, Style::default().fg(Color::Cyan)),
                ]));

                // Product name
                let product_name = get_product_name(config.product, config.mode);
                food_lines.push(Line::from(vec![
                    Span::raw("Product: "),
                    Span::styled(product_name, Style::default().fg(Color::White)),
                ]));

                // Show threshold/target based on mode
                if config.mode == FoodSafeMode::Integrated {
                    // Progress bar for log reduction
                    let progress = data.progress_percent();
                    let bar_width: usize = 12;
                    let filled = ((progress / 100.0) * bar_width as f64) as usize;
                    let empty = bar_width.saturating_sub(filled);
                    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

                    food_lines.push(Line::from(vec![
                        Span::raw("Log Red: "),
                        Span::styled(
                            format!("{:.2}/{:.1}", data.log_reduction, config.target_log_reduction),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]));
                    food_lines.push(Line::from(Span::styled(bar, Style::default().fg(Color::Green))));
                } else {
                    food_lines.push(Line::from(vec![
                        Span::raw("Target: "),
                        Span::styled(
                            app.temperature_unit.format(config.threshold_temperature),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]));
                }

                // Time at temp
                food_lines.push(Line::from(vec![
                    Span::raw("Time@Temp: "),
                    Span::raw(format!("{}s", data.seconds_above_threshold)),
                ]));

                // Serving mode
                let serving_str = match config.serving {
                    Serving::ServedImmediately => "Immediate",
                    Serving::CookedAndChilled => "Chilled",
                };
                food_lines.push(Line::from(vec![
                    Span::raw("Serving: "),
                    Span::styled(serving_str, Style::default().fg(Color::Magenta)),
                ]));
            } else {
                // Legacy display without config
                let status_style = match data.serving_state {
                    FoodSafeServingState::SafeToServe => Style::default().fg(Color::Green),
                    FoodSafeServingState::NotSafe => Style::default().fg(Color::Red),
                };

                food_lines.push(Line::from(vec![
                    Span::raw("Serving: "),
                    Span::styled(format!("{:?}", data.serving_state), status_style),
                ]));

                food_lines.push(Line::from(format!(
                    "Progress: {:.1}%",
                    data.progress_percent()
                )));

                food_lines.push(Line::from(format!(
                    "Time@Temp: {}s",
                    data.seconds_above_threshold
                )));
            }
        } else {
            food_lines.push(Line::from(Span::styled(
                "Not configured",
                Style::default().fg(Color::DarkGray),
            )));
            food_lines.push(Line::from(""));
            food_lines.push(Line::from(vec![
                Span::styled("[F]", Style::default().fg(Color::Yellow)),
                Span::raw(" Configure"),
            ]));
            food_lines.push(Line::from(vec![
                Span::styled("[X]", Style::default().fg(Color::Yellow)),
                Span::raw(" Reset"),
            ]));
        }
    }

    let food_para = Paragraph::new(food_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Food Safety (SafeCook) "),
    );
    frame.render_widget(food_para, chunks[0]);

    // Log sync
    let mut log_lines = vec![];

    if let Some(probe) = app.selected_probe() {
        let percent = probe.percent_of_logs_synced();
        let min_seq = probe.min_sequence_number();
        let max_seq = probe.max_sequence_number();

        log_lines.push(Line::from(format!("Range: {} - {}", min_seq, max_seq)));
        log_lines.push(Line::from(format!("Synced: {:.1}%", percent)));

        // Progress bar
        let bar_width: usize = 15;
        let filled = ((percent / 100.0) * bar_width as f64) as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
        log_lines.push(Line::from(bar));
    }

    let log_para =
        Paragraph::new(log_lines).block(Block::default().borders(Borders::ALL).title(" Log Sync "));
    frame.render_widget(log_para, chunks[1]);
}

fn render_event_log(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .event_log
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|entry| {
            let elapsed = entry.timestamp.elapsed();
            let mins = elapsed.as_secs() / 60;
            let secs = elapsed.as_secs() % 60;

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:02}:{:02} ", mins, secs),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("[{}] ", entry.level.label()), entry.level.style()),
                Span::raw(&entry.message),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Event Log "));

    frame.render_widget(list, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let unit_str = match app.temperature_unit {
        TemperatureUnit::Celsius => "°C",
        TemperatureUnit::Fahrenheit => "°F",
    };

    let status = format!(
        " Probes: {} | Scanning: {} | Unit: {} | Press ? for help ",
        app.probes.len(),
        if app.is_scanning { "Yes" } else { "No" },
        unit_str
    );

    let paragraph =
        Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));

    frame.render_widget(paragraph, area);
}

fn render_dialog(frame: &mut Frame, dialog: &DialogState, area: Rect) {
    // Use larger dialog for FoodSafe
    let dialog_area = match dialog.dialog_type {
        DialogType::SetFoodSafe => centered_rect(60, 60, area),
        DialogType::SetAlarm => centered_rect(55, 50, area),
        _ => centered_rect(50, 40, area),
    };

    // Clear the area
    frame.render_widget(Clear, dialog_area);

    let (title, content) = match &dialog.dialog_type {
        DialogType::SetPrediction => {
            let modes = ["Time to Removal", "Removal and Resting"];
            let mut content = vec![
                Line::from("Enter target temperature (°C):"),
                Line::from(""),
                Line::from(Span::styled(
                    format!("> {}_", dialog.input),
                    if dialog.input_stage == 0 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                )),
                Line::from(""),
                Line::from("Select prediction mode:"),
            ];

            for (i, mode) in modes.iter().enumerate() {
                let is_selected = i == dialog.selected_mode;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if is_selected { "● " } else { "○ " };
                content.push(Line::from(Span::styled(
                    format!("{}{}", prefix, mode),
                    style,
                )));
            }

            content.push(Line::from(""));
            content.push(Line::from("[↑↓] Mode  [Enter] Confirm  [Esc] Cancel"));
            (" Set Prediction ", content)
        }
        DialogType::SetProbeId => {
            let content = vec![
                Line::from("Enter probe ID (1-8):"),
                Line::from(""),
                Line::from(Span::styled(
                    format!("> {}_", dialog.input),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("[Enter] Confirm  [Esc] Cancel"),
            ];
            (" Set Probe ID ", content)
        }
        DialogType::SetFoodSafe => {
            let fs = &dialog.food_safe;
            let mut content = vec![];

            // Stage indicator
            let stage_names = ["Mode", "Product", "Serving"];
            let stage_line = stage_names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    if i == fs.stage {
                        Span::styled(
                            format!("[{}]", name),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else if i < fs.stage {
                        Span::styled(format!("[{}✓]", name), Style::default().fg(Color::Green))
                    } else {
                        Span::styled(format!("[{}]", name), Style::default().fg(Color::DarkGray))
                    }
                })
                .collect::<Vec<_>>();
            content.push(Line::from(stage_line));
            content.push(Line::from(""));

            match fs.stage {
                0 => {
                    // Mode selection
                    content.push(Line::from(Span::styled(
                        "Select Food Safe Mode:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    content.push(Line::from(""));

                    let modes = [
                        ("Simplified", "USDA instant temperature thresholds"),
                        ("Integrated", "Time-temperature log reduction"),
                    ];

                    for (i, (name, desc)) in modes.iter().enumerate() {
                        let is_selected = i == fs.selected_mode;
                        let style = if is_selected {
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        let prefix = if is_selected { "● " } else { "○ " };
                        content.push(Line::from(Span::styled(
                            format!("{}{}", prefix, name),
                            style,
                        )));
                        content.push(Line::from(Span::styled(
                            format!("    {}", desc),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                1 => {
                    // Product selection
                    let mode_name = if fs.selected_mode == 0 {
                        "Simplified"
                    } else {
                        "Integrated"
                    };
                    content.push(Line::from(Span::styled(
                        format!("Select Product ({} Mode):", mode_name),
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    content.push(Line::from(""));

                    let products: Vec<&str> = if fs.selected_mode == 0 {
                        get_simplified_products()
                            .iter()
                            .map(|(_, name)| *name)
                            .collect()
                    } else {
                        get_integrated_products()
                            .iter()
                            .map(|(_, name)| *name)
                            .collect()
                    };

                    // Show scrollable list (max 10 items visible)
                    let start_idx = if fs.selected_product >= 8 {
                        fs.selected_product - 7
                    } else {
                        0
                    };
                    let end_idx = (start_idx + 10).min(products.len());

                    if start_idx > 0 {
                        content.push(Line::from(Span::styled(
                            "  ↑ more above...",
                            Style::default().fg(Color::DarkGray),
                        )));
                    }

                    for (i, product) in products
                        .iter()
                        .enumerate()
                        .skip(start_idx)
                        .take(end_idx - start_idx)
                    {
                        let is_selected = i == fs.selected_product;
                        let style = if is_selected {
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        let prefix = if is_selected { "> " } else { "  " };
                        content.push(Line::from(Span::styled(
                            format!("{}{}", prefix, product),
                            style,
                        )));
                    }

                    if end_idx < products.len() {
                        content.push(Line::from(Span::styled(
                            "  ↓ more below...",
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                2 => {
                    // Serving selection
                    content.push(Line::from(Span::styled(
                        "Select Serving Mode:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    content.push(Line::from(""));

                    let servings = [
                        ("Served Immediately", "Food served right after cooking"),
                        ("Cooked and Chilled", "Food chilled for later use"),
                    ];

                    for (i, (name, desc)) in servings.iter().enumerate() {
                        let is_selected = i == fs.selected_serving;
                        let style = if is_selected {
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        let prefix = if is_selected { "● " } else { "○ " };
                        content.push(Line::from(Span::styled(
                            format!("{}{}", prefix, name),
                            style,
                        )));
                        content.push(Line::from(Span::styled(
                            format!("    {}", desc),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                _ => {}
            }

            content.push(Line::from(""));
            let nav_help = if fs.stage == 2 {
                "[↑↓] Select  [Enter] Confirm  [←] Back  [Esc] Cancel"
            } else {
                "[↑↓] Select  [Enter/→] Next  [←] Back  [Esc] Cancel"
            };
            content.push(Line::from(nav_help));
            (" Configure Food Safety ", content)
        }
        DialogType::SetProbeColor => {
            let colors = [
                ("Yellow", Color::Yellow),
                ("Grey", Color::Gray),
                ("Red", Color::Red),
                ("Orange", Color::LightRed),
                ("Blue", Color::Blue),
                ("Green", Color::Green),
                ("Purple", Color::Magenta),
                ("Pink", Color::LightMagenta),
            ];
            let mut content = vec![Line::from("Select probe color:")];
            content.push(Line::from(""));

            for (i, (name, color)) in colors.iter().enumerate() {
                let style = if i == dialog.selected_option {
                    Style::default().fg(*color).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(*color)
                };
                let prefix = if i == dialog.selected_option {
                    "> "
                } else {
                    "  "
                };
                content.push(Line::from(Span::styled(
                    format!("{}{}", prefix, name),
                    style,
                )));
            }

            content.push(Line::from(""));
            content.push(Line::from("[↑↓] Select  [Enter] Confirm  [Esc] Cancel"));
            (" Set Probe Color ", content)
        }
        DialogType::SetAlarm => {
            let alarm = &dialog.alarm;
            let mut content = vec![];

            let alarm_types = [
                ("Core HIGH alarm", "Alert when core exceeds temperature"),
                ("Core LOW alarm", "Alert when core drops below temperature"),
                ("Surface HIGH alarm", "Alert when surface exceeds temperature"),
                ("Ambient LOW alarm", "Alert when ambient drops below temperature"),
                ("Disable ALL alarms", "Turn off all temperature alarms"),
            ];

            if alarm.stage == 0 {
                // Select alarm type
                content.push(Line::from(Span::styled(
                    "Select Alarm Type:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                content.push(Line::from(""));

                for (i, (name, desc)) in alarm_types.iter().enumerate() {
                    let is_selected = i == alarm.selected_alarm_type;
                    let style = if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    let prefix = if is_selected { "> " } else { "  " };
                    content.push(Line::from(Span::styled(
                        format!("{}{}", prefix, name),
                        style,
                    )));
                    content.push(Line::from(Span::styled(
                        format!("    {}", desc),
                        Style::default().fg(Color::DarkGray),
                    )));
                }

                content.push(Line::from(""));
                content.push(Line::from("[↑↓] Select  [Enter] Next  [Esc] Cancel"));
            } else {
                // Enter temperature (for types 0-3)
                let alarm_name = alarm_types[alarm.selected_alarm_type].0;
                content.push(Line::from(Span::styled(
                    format!("Configure: {}", alarm_name),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                content.push(Line::from(""));
                content.push(Line::from("Enter temperature threshold (°C):"));
                content.push(Line::from(""));
                content.push(Line::from(Span::styled(
                    format!("> {}_", alarm.temp_input),
                    Style::default().fg(Color::Yellow),
                )));
                content.push(Line::from(""));
                content.push(Line::from("Common values:"));
                content.push(Line::from("  74°C (165°F) - Poultry safe"));
                content.push(Line::from("  63°C (145°F) - Beef/Pork safe"));
                content.push(Line::from("   4°C (40°F)  - Refrigeration"));
                content.push(Line::from(""));
                content.push(Line::from("[Enter] Confirm  [←] Back  [Esc] Cancel"));
            }

            (" Configure Temperature Alarm ", content)
        }
        DialogType::ConfirmReset => {
            let content = vec![
                Line::from(Span::styled(
                    "⚠ WARNING: Reset Thermometer",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("This will reset the thermometer to factory defaults."),
                Line::from(""),
                Line::from("The following will be reset:"),
                Line::from("  • Probe ID"),
                Line::from("  • Probe color"),
                Line::from("  • All temperature alarms"),
                Line::from("  • Power mode settings"),
                Line::from(""),
                Line::from(Span::styled(
                    "This action cannot be undone!",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("[Enter] Confirm Reset  [Esc] Cancel"),
            ];
            (" Confirm Reset ", content)
        }
        DialogType::Help => {
            let content = vec![
                Line::from("Keyboard Shortcuts:"),
                Line::from(""),
                Line::from("  ↑/↓     Navigate probe list"),
                Line::from("  Enter   Connect/disconnect probe"),
                Line::from("  P       Set prediction target"),
                Line::from("  C       Cancel prediction"),
                Line::from("  F       Configure food safety"),
                Line::from("  X       Reset food safety"),
                Line::from("  A       Configure temperature alarms"),
                Line::from("  M       Silence alarms"),
                Line::from("  W       Toggle power mode"),
                Line::from("  R       Reset thermometer"),
                Line::from("  I       Set probe ID (1-8)"),
                Line::from("  O       Set probe color"),
                Line::from("  E       Export logs to CSV"),
                Line::from("  S       Start/stop scanning"),
                Line::from("  U       Toggle temperature units"),
                Line::from("  ?       Show this help"),
                Line::from("  Q/Esc   Quit"),
            ];
            (" Help ", content)
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title)
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(content).block(block);

    frame.render_widget(paragraph, dialog_area);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let help_area = centered_rect(60, 80, area);

    frame.render_widget(Clear, help_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Combustion Probe Dashboard",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  ↑/↓        Navigate probe list"),
        Line::from("  Enter      Connect/disconnect selected probe"),
        Line::from(""),
        Line::from("Temperature & Prediction:"),
        Line::from("  P          Set prediction target temperature"),
        Line::from("  C          Cancel active prediction"),
        Line::from("  U          Toggle Celsius/Fahrenheit"),
        Line::from(""),
        Line::from("Food Safety:"),
        Line::from("  F          Configure food safety (select product)"),
        Line::from("  X          Reset food safety calculations"),
        Line::from(""),
        Line::from(Span::styled(
            "Temperature Alarms:",
            Style::default().fg(Color::Cyan),
        )),
        Line::from("  A          Configure temperature alarms"),
        Line::from("  M          Silence currently sounding alarms"),
        Line::from(""),
        Line::from(Span::styled(
            "Power & Reset:",
            Style::default().fg(Color::Cyan),
        )),
        Line::from("  W          Toggle power mode (Normal/Always On)"),
        Line::from("  R          Reset thermometer to factory defaults"),
        Line::from(""),
        Line::from("Probe Configuration:"),
        Line::from("  I          Set probe ID (1-8)"),
        Line::from("  O          Cycle probe color"),
        Line::from(""),
        Line::from("Logging:"),
        Line::from("  E          Export temperature logs to CSV"),
        Line::from(""),
        Line::from("Scanning:"),
        Line::from("  S          Start/stop BLE scanning"),
        Line::from(""),
        Line::from("Application:"),
        Line::from("  ?          Toggle this help screen"),
        Line::from("  Q/Esc      Quit application"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Help ")
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(content).block(block);

    frame.render_widget(paragraph, help_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn color_emoji(color: ProbeColor) -> &'static str {
    match color {
        ProbeColor::Yellow => "🟡",
        ProbeColor::Grey => "⚪",
        ProbeColor::Red => "🔴",
        ProbeColor::Orange => "🟠",
        ProbeColor::Blue => "🔵",
        ProbeColor::Green => "🟢",
        ProbeColor::Purple => "🟣",
        ProbeColor::Pink => "🩷",
    }
}

async fn run_app(terminal: &mut Terminal, mut app: App) -> Result<()> {
    // Start scanning
    app.start_scanning().await?;

    loop {
        // Update probe list
        app.update_probes();

        // Draw UI
        terminal
            .draw(|frame| render_ui(frame, &app))
            .map_err(|e| combustion_rust_ble::Error::Internal(format!("Draw error: {}", e)))?;

        // Handle input with timeout for updates
        let has_event = event::poll(Duration::from_millis(100))
            .map_err(|e| combustion_rust_ble::Error::Internal(format!("Poll error: {}", e)))?;

        if has_event {
            let event = event::read()
                .map_err(|e| combustion_rust_ble::Error::Internal(format!("Read error: {}", e)))?;

            if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press {
                    // Handle dialog input first
                    if let Some(ref mut dialog) = app.dialog {
                        match key.code {
                            KeyCode::Esc => {
                                app.close_dialog();
                            }
                            KeyCode::Enter | KeyCode::Right => {
                                // For FoodSafe, Enter/Right advances to next stage
                                if matches!(dialog.dialog_type, DialogType::SetFoodSafe) {
                                    if dialog.food_safe.stage < 2 {
                                        dialog.food_safe.stage += 1;
                                        // Reset product selection when changing mode
                                        if dialog.food_safe.stage == 1 {
                                            dialog.food_safe.selected_product = 0;
                                        }
                                    } else {
                                        // Final stage - confirm
                                        let _ = app.handle_dialog_confirm().await;
                                    }
                                } else if matches!(dialog.dialog_type, DialogType::SetAlarm) {
                                    if dialog.alarm.stage == 0 {
                                        // If "Disable All" is selected, confirm immediately
                                        if dialog.alarm.selected_alarm_type == 4 {
                                            let _ = app.handle_dialog_confirm().await;
                                        } else {
                                            // Otherwise, go to temperature input stage
                                            dialog.alarm.stage = 1;
                                        }
                                    } else {
                                        // Confirm the alarm
                                        let _ = app.handle_dialog_confirm().await;
                                    }
                                } else {
                                    let _ = app.handle_dialog_confirm().await;
                                }
                            }
                            KeyCode::Left => {
                                // For FoodSafe, go back to previous stage
                                if matches!(dialog.dialog_type, DialogType::SetFoodSafe) {
                                    if dialog.food_safe.stage > 0 {
                                        dialog.food_safe.stage -= 1;
                                    }
                                } else if matches!(dialog.dialog_type, DialogType::SetAlarm) {
                                    if dialog.alarm.stage > 0 {
                                        dialog.alarm.stage -= 1;
                                    }
                                }
                            }
                            KeyCode::Char(c) => {
                                if matches!(dialog.dialog_type, DialogType::SetAlarm)
                                    && dialog.alarm.stage == 1
                                {
                                    // Alarm temp input
                                    if c.is_ascii_digit() || c == '.' || c == '-' {
                                        dialog.alarm.temp_input.push(c);
                                    }
                                } else {
                                    dialog.input.push(c);
                                }
                            }
                            KeyCode::Backspace => {
                                if matches!(dialog.dialog_type, DialogType::SetAlarm)
                                    && dialog.alarm.stage == 1
                                {
                                    dialog.alarm.temp_input.pop();
                                } else {
                                    dialog.input.pop();
                                }
                            }
                            KeyCode::Up => match dialog.dialog_type {
                                DialogType::SetPrediction => {
                                    if dialog.selected_mode > 0 {
                                        dialog.selected_mode -= 1;
                                    }
                                }
                                DialogType::SetAlarm => {
                                    if dialog.alarm.stage == 0
                                        && dialog.alarm.selected_alarm_type > 0
                                    {
                                        dialog.alarm.selected_alarm_type -= 1;
                                    }
                                }
                                DialogType::SetFoodSafe => {
                                    // Navigate within current stage
                                    match dialog.food_safe.stage {
                                        0 => {
                                            // Mode selection
                                            if dialog.food_safe.selected_mode > 0 {
                                                dialog.food_safe.selected_mode -= 1;
                                            }
                                        }
                                        1 => {
                                            // Product selection
                                            if dialog.food_safe.selected_product > 0 {
                                                dialog.food_safe.selected_product -= 1;
                                            }
                                        }
                                        2 => {
                                            // Serving selection
                                            if dialog.food_safe.selected_serving > 0 {
                                                dialog.food_safe.selected_serving -= 1;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {
                                    if dialog.selected_option > 0 {
                                        dialog.selected_option -= 1;
                                    }
                                }
                            },
                            KeyCode::Down => match dialog.dialog_type {
                                DialogType::SetPrediction => {
                                    if dialog.selected_mode < 1 {
                                        dialog.selected_mode += 1;
                                    }
                                }
                                DialogType::SetAlarm => {
                                    if dialog.alarm.stage == 0
                                        && dialog.alarm.selected_alarm_type < 4
                                    {
                                        dialog.alarm.selected_alarm_type += 1;
                                    }
                                }
                                DialogType::SetFoodSafe => {
                                    // Navigate within current stage
                                    match dialog.food_safe.stage {
                                        0 => {
                                            // Mode selection (2 options)
                                            if dialog.food_safe.selected_mode < 1 {
                                                dialog.food_safe.selected_mode += 1;
                                            }
                                        }
                                        1 => {
                                            // Product selection
                                            let max_products = if dialog.food_safe.selected_mode == 0
                                            {
                                                get_simplified_products().len() - 1
                                            } else {
                                                get_integrated_products().len() - 1
                                            };
                                            if dialog.food_safe.selected_product < max_products {
                                                dialog.food_safe.selected_product += 1;
                                            }
                                        }
                                        2 => {
                                            // Serving selection (2 options)
                                            if dialog.food_safe.selected_serving < 1 {
                                                dialog.food_safe.selected_serving += 1;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                DialogType::SetProbeColor => {
                                    if dialog.selected_option < 7 {
                                        dialog.selected_option += 1;
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                        continue;
                    }

                    // Handle help overlay
                    if app.show_help {
                        app.show_help = false;
                        continue;
                    }

                    // Handle normal input
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            break;
                        }
                        KeyCode::Char('?') => {
                            app.show_help = true;
                        }
                        KeyCode::Up => {
                            app.select_prev_probe();
                        }
                        KeyCode::Down => {
                            app.select_next_probe();
                        }
                        KeyCode::Enter => {
                            let _ = app.toggle_connection().await;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            let _ = app.toggle_scanning().await;
                        }
                        KeyCode::Char('u') | KeyCode::Char('U') => {
                            app.toggle_unit();
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            app.open_dialog(DialogType::SetPrediction);
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            let _ = app.cancel_prediction().await;
                        }
                        KeyCode::Char('f') | KeyCode::Char('F') => {
                            app.open_dialog(DialogType::SetFoodSafe);
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') => {
                            let _ = app.reset_food_safe().await;
                        }
                        KeyCode::Char('i') | KeyCode::Char('I') => {
                            app.open_dialog(DialogType::SetProbeId);
                        }
                        KeyCode::Char('o') | KeyCode::Char('O') => {
                            app.open_dialog(DialogType::SetProbeColor);
                        }
                        KeyCode::Char('e') | KeyCode::Char('E') => {
                            app.export_logs();
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            app.open_dialog(DialogType::SetAlarm);
                        }
                        KeyCode::Char('m') | KeyCode::Char('M') => {
                            let _ = app.silence_alarms().await;
                        }
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            let _ = app.toggle_power_mode().await;
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app.open_dialog(DialogType::ConfirmReset);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    app.shutdown().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to file (so it doesn't interfere with TUI)
    let _log_file = std::fs::File::create("probe_dashboard.log").ok();

    // Setup terminal
    let mut terminal = setup_terminal().map_err(|e| {
        combustion_rust_ble::Error::Internal(format!("Failed to setup terminal: {}", e))
    })?;

    // Create app
    let app = App::new().await?;

    // Run the app
    let result = run_app(&mut terminal, app).await;

    // Restore terminal
    let _ = restore_terminal(&mut terminal);

    result
}
