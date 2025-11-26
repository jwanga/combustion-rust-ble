//! Comprehensive TUI dashboard for Combustion probe monitoring and debugging
//!
//! Run with: cargo run --example probe_dashboard
//!
//! This example provides a full-featured terminal interface for:
//! - Discovering and monitoring multiple probes simultaneously
//! - Real-time temperature visualization
//! - Prediction and food safety configuration
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
//! | `I` | Set probe ID |
//! | `O` | Cycle probe color |
//! | `L` | Download logs |
//! | `E` | Export logs to CSV |
//! | `S` | Start/stop scanning |
//! | `U` | Toggle temperature units |
//! | `?` | Show help |
//! | `Q/Esc` | Quit |

use combustion_rust_ble::{
    celsius_to_fahrenheit, BatteryStatus, ConnectionState, DeviceManager, FoodSafeProduct,
    FoodSafeServingState, PredictionMode, PredictionState, PredictionType, Probe, ProbeColor,
    ProbeMode, Result,
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
    Help,
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
                    let products = [
                        FoodSafeProduct::ChickenBreast,
                        FoodSafeProduct::GroundBeef,
                        FoodSafeProduct::BeefSteak,
                        FoodSafeProduct::PorkChop,
                        FoodSafeProduct::Salmon,
                    ];
                    if let Some(product) = products.get(dialog.selected_option) {
                        if let Some(probe) = self.selected_probe().cloned() {
                            probe.configure_food_safe(*product).await?;
                            self.log(
                                LogLevel::Info,
                                format!("Configured food safety: {:?}", product),
                            );
                        }
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

    let actions = vec![
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" Connect/Disconnect"),
        ]),
        Line::from(vec![
            Span::styled("[P]", Style::default().fg(Color::Yellow)),
            Span::raw(" Set Prediction"),
            if connected {
                Span::raw("")
            } else {
                Span::styled(
                    " (requires connection)",
                    Style::default().fg(Color::DarkGray),
                )
            },
        ]),
        Line::from(vec![
            Span::styled("[C]", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel Prediction  "),
            Span::styled("[F]", Style::default().fg(Color::Yellow)),
            Span::raw(" Food Safety"),
        ]),
        Line::from(vec![
            Span::styled("[I]", Style::default().fg(Color::Yellow)),
            Span::raw(" Set ID  "),
            Span::styled("[O]", Style::default().fg(Color::Yellow)),
            Span::raw(" Cycle Color  "),
            Span::styled("[X]", Style::default().fg(Color::Yellow)),
            Span::raw(" Reset Safe"),
        ]),
        Line::from(vec![
            Span::styled("[E]", Style::default().fg(Color::Yellow)),
            Span::raw(" Export Logs  "),
            Span::styled("[S]", Style::default().fg(Color::Yellow)),
            Span::raw(" Toggle Scan  "),
            Span::styled("[U]", Style::default().fg(Color::Yellow)),
            Span::raw(" Units"),
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
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Food safety
    let mut food_lines = vec![];

    if let Some(probe) = app.selected_probe() {
        if let Some(data) = probe.food_safe_data() {
            let status_style = match data.serving_state {
                FoodSafeServingState::SafeToServe => Style::default().fg(Color::Green),
                FoodSafeServingState::NotSafe => Style::default().fg(Color::Red),
            };

            let status_icon = match data.serving_state {
                FoodSafeServingState::SafeToServe => "✓ SAFE",
                FoodSafeServingState::NotSafe => "⚠ NOT SAFE",
            };

            food_lines.push(Line::from(vec![
                Span::raw("Status: "),
                Span::styled(status_icon, status_style),
            ]));

            food_lines.push(Line::from(format!(
                "Log Reduction: {:.2} / {:.1}",
                data.log_reduction,
                data.progress_percent()
            )));

            food_lines.push(Line::from(format!(
                "Time at Temp: {}s",
                data.seconds_above_threshold
            )));
        } else {
            food_lines.push(Line::from("Not configured"));
            food_lines.push(Line::from("Press [F] to set"));
        }
    }

    let food_para = Paragraph::new(food_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Food Safety "),
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
    let dialog_area = centered_rect(50, 40, area);

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
            let products = [
                "Chicken Breast",
                "Ground Beef",
                "Beef Steak",
                "Pork Chop",
                "Salmon",
            ];
            let mut content = vec![Line::from("Select food product:")];
            content.push(Line::from(""));

            for (i, product) in products.iter().enumerate() {
                let style = if i == dialog.selected_option {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if i == dialog.selected_option {
                    "> "
                } else {
                    "  "
                };
                content.push(Line::from(Span::styled(
                    format!("{}{}", prefix, product),
                    style,
                )));
            }

            content.push(Line::from(""));
            content.push(Line::from("[↑↓] Select  [Enter] Confirm  [Esc] Cancel"));
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
    let help_area = centered_rect(60, 70, area);

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
                            KeyCode::Enter => {
                                let _ = app.handle_dialog_confirm().await;
                            }
                            KeyCode::Char(c) => {
                                dialog.input.push(c);
                            }
                            KeyCode::Backspace => {
                                dialog.input.pop();
                            }
                            KeyCode::Up => match dialog.dialog_type {
                                DialogType::SetPrediction => {
                                    if dialog.selected_mode > 0 {
                                        dialog.selected_mode -= 1;
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
                                DialogType::SetFoodSafe => {
                                    if dialog.selected_option < 4 {
                                        dialog.selected_option += 1;
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
