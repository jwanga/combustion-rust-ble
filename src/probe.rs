//! Probe struct and methods.
//!
//! Represents a single Combustion Predictive Thermometer probe.

use btleplug::platform::Peripheral;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::info;

use crate::ble::advertising::{
    AdvertisingData, BatteryStatus, Overheating, ProbeColor, ProbeId, ProbeMode,
};
use crate::ble::characteristics::CharacteristicHandler;
use crate::ble::connection::{ConnectionManager, ConnectionState};
use crate::ble::uuids::*;
use crate::data::{
    AlarmConfig, FoodSafeConfig, FoodSafeData, FoodSafeProduct, PowerMode, PredictionInfo,
    PredictionMode, ProbeTemperatures, Serving, SessionInfo, TemperatureLog,
    ThermometerPreferences, VirtualTemperatures,
};
use crate::error::{Error, Result};
use crate::protocol::uart_messages::*;
use crate::protocol::ProbeStatus;

/// Callback handle for unregistering callbacks.
pub struct CallbackHandle {
    id: u64,
    unregister_fn: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl CallbackHandle {
    /// Create a new callback handle.
    pub(crate) fn new(id: u64, unregister_fn: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self {
            id,
            unregister_fn: Some(Box::new(unregister_fn)),
        }
    }

    /// Unregister this callback.
    pub fn unregister(mut self) {
        if let Some(f) = self.unregister_fn.take() {
            f();
        }
    }

    /// Get the callback ID.
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for CallbackHandle {
    fn drop(&mut self) {
        if let Some(f) = self.unregister_fn.take() {
            f();
        }
    }
}

/// Grace period after setting ID/color before accepting advertising updates.
/// This allows time for the probe to process the command and start advertising new values.
const ID_COLOR_GRACE_PERIOD: Duration = Duration::from_secs(5);

/// Internal state for a probe.
struct ProbeState {
    /// Serial number.
    serial_number: u32,
    /// Probe ID (1-8).
    probe_id: ProbeId,
    /// Probe color.
    color: ProbeColor,
    /// Time when probe ID was last explicitly set (to ignore stale advertising data).
    probe_id_set_at: Option<Instant>,
    /// Time when probe color was last explicitly set (to ignore stale advertising data).
    color_set_at: Option<Instant>,
    /// Current temperatures.
    temperatures: ProbeTemperatures,
    /// Virtual temperatures.
    virtual_temperatures: VirtualTemperatures,
    /// Prediction info.
    prediction: Option<PredictionInfo>,
    /// Battery status.
    battery_status: BatteryStatus,
    /// Probe mode.
    mode: ProbeMode,
    /// Overheating info.
    overheating: Overheating,
    /// Min sequence number.
    min_sequence: u32,
    /// Max sequence number.
    max_sequence: u32,
    /// Temperature log.
    temperature_log: TemperatureLog,
    /// Food safety data.
    food_safe_data: Option<FoodSafeData>,
    /// Session info.
    session_info: Option<SessionInfo>,
    /// RSSI value.
    rssi: Option<i16>,
    /// Last update time.
    last_update: Instant,
    /// Thermometer preferences (power mode).
    thermometer_preferences: Option<ThermometerPreferences>,
    /// Alarm configuration.
    alarm_config: Option<AlarmConfig>,
}

impl ProbeState {
    fn new(serial_number: u32) -> Self {
        Self {
            serial_number,
            probe_id: ProbeId::default(),
            color: ProbeColor::default(),
            probe_id_set_at: None,
            color_set_at: None,
            temperatures: ProbeTemperatures::new(),
            virtual_temperatures: VirtualTemperatures::default(),
            prediction: None,
            battery_status: BatteryStatus::default(),
            mode: ProbeMode::default(),
            overheating: Overheating::default(),
            min_sequence: 0,
            max_sequence: 0,
            temperature_log: TemperatureLog::default(),
            food_safe_data: None,
            session_info: None,
            rssi: None,
            last_update: Instant::now(),
            thermometer_preferences: None,
            alarm_config: None,
        }
    }
}

/// Temperature update event.
#[derive(Debug, Clone)]
pub struct TemperatureUpdate {
    /// Raw temperatures.
    pub temperatures: ProbeTemperatures,
    /// Virtual temperatures.
    pub virtual_temperatures: VirtualTemperatures,
}

/// Represents a single Combustion Predictive Thermometer probe.
pub struct Probe {
    /// BLE identifier.
    identifier: String,
    /// Internal state.
    state: Arc<RwLock<ProbeState>>,
    /// Connection manager.
    connection: Arc<ConnectionManager>,
    /// Characteristic handler.
    characteristics: Arc<RwLock<Option<CharacteristicHandler>>>,
    /// Whether the probe is stale.
    is_stale: Arc<AtomicBool>,
    /// Temperature update channel.
    temperature_tx: broadcast::Sender<TemperatureUpdate>,
    /// Prediction update channel.
    prediction_tx: broadcast::Sender<PredictionInfo>,
    /// Log sync progress channel.
    log_sync_tx: broadcast::Sender<f64>,
    /// Stale timeout.
    stale_timeout: Duration,
    /// Callback ID counter.
    callback_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl Probe {
    /// Default stale timeout (15 seconds).
    pub const DEFAULT_STALE_TIMEOUT: Duration = Duration::from_secs(15);

    /// Create a new probe instance.
    pub(crate) fn new(identifier: String, peripheral: Peripheral, serial_number: u32) -> Self {
        let (temperature_tx, _) = broadcast::channel(64);
        let (prediction_tx, _) = broadcast::channel(16);
        let (log_sync_tx, _) = broadcast::channel(16);

        Self {
            identifier,
            state: Arc::new(RwLock::new(ProbeState::new(serial_number))),
            connection: Arc::new(ConnectionManager::new(peripheral)),
            characteristics: Arc::new(RwLock::new(None)),
            is_stale: Arc::new(AtomicBool::new(false)),
            temperature_tx,
            prediction_tx,
            log_sync_tx,
            stale_timeout: Self::DEFAULT_STALE_TIMEOUT,
            callback_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Update from advertising data.
    pub(crate) fn update_from_advertising(&self, adv_data: &AdvertisingData, rssi: Option<i16>) {
        let mut state = self.state.write();
        let now = Instant::now();

        state.temperatures = adv_data.temperatures.clone();
        state.virtual_temperatures = adv_data.virtual_temperatures.clone();

        // Only update probe_id from advertising if we haven't recently set it explicitly.
        // This prevents stale advertising packets from overwriting a pending ID change.
        let id_in_grace_period = state
            .probe_id_set_at
            .map(|t| now.duration_since(t) < ID_COLOR_GRACE_PERIOD)
            .unwrap_or(false);
        if !id_in_grace_period {
            state.probe_id = adv_data.probe_id;
        }

        // Only update color from advertising if we haven't recently set it explicitly.
        let color_in_grace_period = state
            .color_set_at
            .map(|t| now.duration_since(t) < ID_COLOR_GRACE_PERIOD)
            .unwrap_or(false);
        if !color_in_grace_period {
            state.color = adv_data.color;
        }

        state.battery_status = adv_data.battery_status;
        state.mode = adv_data.mode;
        state.overheating = Overheating::new(adv_data.overheating_sensors);
        state.rssi = rssi;
        state.last_update = now;

        // Reset stale flag
        self.is_stale.store(false, Ordering::SeqCst);

        // Send temperature update
        let _ = self.temperature_tx.send(TemperatureUpdate {
            temperatures: state.temperatures.clone(),
            virtual_temperatures: state.virtual_temperatures.clone(),
        });
    }

    /// Update from status notification.
    #[allow(dead_code)]
    pub(crate) fn update_from_status(&self, status: &ProbeStatus) {
        let mut state = self.state.write();
        let now = Instant::now();

        state.temperatures = status.temperatures.clone();
        state.virtual_temperatures = status.virtual_temperatures.clone();

        // Only update probe_id from status if we haven't recently set it explicitly.
        let id_in_grace_period = state
            .probe_id_set_at
            .map(|t| now.duration_since(t) < ID_COLOR_GRACE_PERIOD)
            .unwrap_or(false);
        if !id_in_grace_period {
            state.probe_id = status.probe_id;
        }

        // Only update color from status if we haven't recently set it explicitly.
        let color_in_grace_period = state
            .color_set_at
            .map(|t| now.duration_since(t) < ID_COLOR_GRACE_PERIOD)
            .unwrap_or(false);
        if !color_in_grace_period {
            state.color = status.color;
        }

        state.battery_status = status.battery_status;
        state.mode = status.mode;
        state.overheating = status.overheating;
        state.min_sequence = status.min_sequence_number;
        state.max_sequence = status.max_sequence_number;
        state.prediction = status.prediction.clone();
        state.last_update = now;

        // Reset stale flag
        self.is_stale.store(false, Ordering::SeqCst);

        // Send updates
        let _ = self.temperature_tx.send(TemperatureUpdate {
            temperatures: state.temperatures.clone(),
            virtual_temperatures: state.virtual_temperatures.clone(),
        });

        if let Some(ref prediction) = state.prediction {
            let _ = self.prediction_tx.send(prediction.clone());
        }
    }

    // === Identification ===

    /// Get the unique serial number.
    pub fn serial_number(&self) -> u32 {
        self.state.read().serial_number
    }

    /// Get the serial number as a formatted string.
    pub fn serial_number_string(&self) -> String {
        format!("{:08X}", self.state.read().serial_number)
    }

    /// Get the BLE identifier.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Get the probe ID (1-8).
    pub fn id(&self) -> ProbeId {
        self.state.read().probe_id
    }

    /// Get the silicone ring color.
    pub fn color(&self) -> ProbeColor {
        self.state.read().color
    }

    // === Connection ===

    /// Get the current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        self.connection.state()
    }

    /// Get the signal strength (RSSI).
    pub fn rssi(&self) -> Option<i16> {
        self.state.read().rssi
    }

    /// Attempt to connect to the probe.
    pub async fn connect(&self) -> Result<()> {
        info!("Connecting to probe {}", self.serial_number_string());

        self.connection.connect(true).await?;

        info!("Connected to probe {}", self.serial_number_string());

        // Set up characteristics handler
        let handler = CharacteristicHandler::new(self.connection.peripheral().clone());
        handler.discover_characteristics().await?;

        // Subscribe to UART notifications
        if handler.has_characteristic(&UART_TX_UUID) {
            handler.subscribe(&UART_TX_UUID).await?;
        }

        // Subscribe to Probe Status notifications for prediction data
        info!(
            "Checking for Probe Status characteristic: {}",
            PROBE_STATUS_CHARACTERISTIC_UUID
        );
        if handler.has_characteristic(&PROBE_STATUS_CHARACTERISTIC_UUID) {
            handler.subscribe(&PROBE_STATUS_CHARACTERISTIC_UUID).await?;
            info!("Subscribed to Probe Status characteristic - prediction data will be available");
        } else {
            info!("Probe Status characteristic NOT found - prediction data will not be available");
        }

        handler.start_notifications().await?;

        // Start processing status notifications
        self.start_status_notification_handler(&handler);

        *self.characteristics.write() = Some(handler);

        Ok(())
    }

    /// Start a background task to process status notifications.
    fn start_status_notification_handler(&self, handler: &CharacteristicHandler) {
        use tracing::debug;

        let mut rx = handler.subscribe_notifications();
        let state = self.state.clone();
        let temperature_tx = self.temperature_tx.clone();
        let prediction_tx = self.prediction_tx.clone();
        let is_stale = self.is_stale.clone();

        let expected_status_uuid = PROBE_STATUS_CHARACTERISTIC_UUID;
        debug!(
            "Status notification handler: Looking for UUID {}",
            expected_status_uuid
        );

        tokio::spawn(async move {
            debug!("Status notification handler started");
            while let Ok(event) = rx.recv().await {
                let is_status = event.characteristic_uuid == expected_status_uuid;
                debug!(
                    "Received notification: UUID={}, expected={}, match={}, data_len={}",
                    event.characteristic_uuid,
                    expected_status_uuid,
                    is_status,
                    event.data.len()
                );

                // Only process probe status notifications
                if is_status {
                    debug!(
                        "Processing Probe Status notification: {} bytes, data: {:02X?}",
                        event.data.len(),
                        &event.data[..std::cmp::min(event.data.len(), 40)]
                    );

                    match ProbeStatus::parse(&event.data) {
                        Ok(status) => {
                            debug!(
                                "Parsed status: prediction={:?}, food_safe_status={:?}",
                                status.prediction.as_ref().map(|p| format!(
                                    "state={:?}, mode={:?}, setpoint={:.1}",
                                    p.state, p.mode, p.set_point_temperature
                                )),
                                status.food_safe_status.as_ref().map(|fs| format!(
                                    "state={:?}, log_red={:.1}, secs={}",
                                    fs.state, fs.log_reduction, fs.seconds_above_threshold
                                ))
                            );

                            let mut state = state.write();
                            let now = Instant::now();

                            state.temperatures = status.temperatures.clone();
                            state.virtual_temperatures = status.virtual_temperatures.clone();
                            state.battery_status = status.battery_status;
                            state.mode = status.mode;
                            state.overheating = status.overheating;
                            state.min_sequence = status.min_sequence_number;
                            state.max_sequence = status.max_sequence_number;
                            state.prediction = status.prediction.clone();

                            // Update thermometer preferences and alarm config from status
                            state.thermometer_preferences = status.thermometer_preferences;
                            state.alarm_config = status.alarm_config.clone();

                            // Update food safe data from status
                            // Handle both local and external (e.g., iOS app) food safe configuration
                            match (&status.food_safe_config, &status.food_safe_status) {
                                (Some(config), Some(fs_status)) => {
                                    if let Some(ref mut food_safe_data) = state.food_safe_data {
                                        // Update existing data with new status
                                        food_safe_data.update_from_status(fs_status.clone());
                                        // Also update config in case it changed externally
                                        food_safe_data.update_config(config.clone());
                                    } else {
                                        // Create new food safe data from external config/status
                                        state.food_safe_data = Some(FoodSafeData::from_config_and_status(
                                            config.clone(),
                                            fs_status.clone(),
                                        ));
                                    }
                                }
                                (Some(config), None) => {
                                    // Config but no status yet - create data with config only
                                    if state.food_safe_data.is_none() {
                                        state.food_safe_data = Some(FoodSafeData::with_config(config.clone()));
                                    } else if let Some(ref mut food_safe_data) = state.food_safe_data {
                                        food_safe_data.update_config(config.clone());
                                    }
                                }
                                (None, Some(fs_status)) => {
                                    // Status but no config - update if we have existing data
                                    if let Some(ref mut food_safe_data) = state.food_safe_data {
                                        food_safe_data.update_from_status(fs_status.clone());
                                    }
                                }
                                (None, None) => {
                                    // No food safe data - clear if not configured locally
                                    // Don't clear here as it might have been set locally
                                }
                            }

                            state.last_update = now;

                            // Reset stale flag
                            is_stale.store(false, Ordering::SeqCst);

                            // Send temperature update
                            let _ = temperature_tx.send(TemperatureUpdate {
                                temperatures: state.temperatures.clone(),
                                virtual_temperatures: state.virtual_temperatures.clone(),
                            });

                            // Send prediction update if available
                            if let Some(ref prediction) = state.prediction {
                                let _ = prediction_tx.send(prediction.clone());
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse status notification: {:?}", e);
                        }
                    }
                }
            }
            debug!("Status notification handler stopped");
        });
    }

    /// Disconnect from the probe.
    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from probe {}", self.serial_number_string());

        // Stop notifications
        if let Some(ref handler) = *self.characteristics.read() {
            handler.stop_notifications().await;
        }

        self.connection.disconnect().await?;
        *self.characteristics.write() = None;

        Ok(())
    }

    /// Check if we're maintaining a connection.
    pub fn is_maintaining_connection(&self) -> bool {
        self.connection.is_maintaining_connection()
    }

    /// Check if the probe is stale (no data received recently).
    pub fn is_stale(&self) -> bool {
        let elapsed = self.state.read().last_update.elapsed();
        let is_stale = elapsed > self.stale_timeout;
        self.is_stale.store(is_stale, Ordering::SeqCst);
        is_stale
    }

    // === Temperature Data ===

    /// Get current temperatures from all 8 sensors.
    pub fn current_temperatures(&self) -> ProbeTemperatures {
        self.state.read().temperatures.clone()
    }

    /// Get virtual temperatures (core, surface, ambient).
    pub fn virtual_temperatures(&self) -> VirtualTemperatures {
        self.state.read().virtual_temperatures.clone()
    }

    /// Subscribe to temperature updates.
    pub fn subscribe_temperatures(&self) -> broadcast::Receiver<TemperatureUpdate> {
        self.temperature_tx.subscribe()
    }

    /// Register a callback for temperature updates.
    pub fn on_temperatures_updated<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(&ProbeTemperatures, &VirtualTemperatures) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.temperature_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(update) = rx.recv().await {
                callback(&update.temperatures, &update.virtual_temperatures);
            }
        });

        CallbackHandle::new(callback_id, move || {
            handle.abort();
        })
    }

    // === Logging ===

    /// Get the minimum sequence number of logs on probe.
    pub fn min_sequence_number(&self) -> u32 {
        self.state.read().min_sequence
    }

    /// Get the maximum sequence number of logs on probe.
    pub fn max_sequence_number(&self) -> u32 {
        self.state.read().max_sequence
    }

    /// Get the percentage of logs synced.
    pub fn percent_of_logs_synced(&self) -> f64 {
        let state = self.state.read();
        state
            .temperature_log
            .percent_synced(state.min_sequence, state.max_sequence)
    }

    /// Access the temperature log.
    pub fn temperature_log(&self) -> TemperatureLog {
        self.state.read().temperature_log.clone()
    }

    /// Subscribe to log sync progress updates.
    pub fn subscribe_log_sync(&self) -> broadcast::Receiver<f64> {
        self.log_sync_tx.subscribe()
    }

    /// Register a callback for log sync progress.
    pub fn on_log_sync_progress<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.log_sync_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(progress) = rx.recv().await {
                callback(progress);
            }
        });

        CallbackHandle::new(callback_id, move || {
            handle.abort();
        })
    }

    // === Prediction ===

    /// Get current prediction information.
    pub fn prediction_info(&self) -> Option<PredictionInfo> {
        self.state.read().prediction.clone()
    }

    /// Set prediction target temperature and mode.
    pub async fn set_prediction(&self, mode: PredictionMode, set_point_celsius: f64) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        if !(0.0..=300.0).contains(&set_point_celsius) {
            return Err(Error::InvalidParameter {
                name: "set_point_celsius".to_string(),
                value: set_point_celsius.to_string(),
            });
        }

        // Per spec: Prediction Set Point = raw * 0.1°C, so raw = celsius * 10
        let set_point_raw = (set_point_celsius * 10.0) as u16;
        let message = build_set_prediction_request(mode.to_raw(), set_point_raw);

        self.send_uart_message(&message).await
    }

    /// Cancel active prediction.
    pub async fn cancel_prediction(&self) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_cancel_prediction_request();
        self.send_uart_message(&message).await
    }

    /// Subscribe to prediction updates.
    pub fn subscribe_predictions(&self) -> broadcast::Receiver<PredictionInfo> {
        self.prediction_tx.subscribe()
    }

    /// Register a callback for prediction updates.
    pub fn on_prediction_updated<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(&PredictionInfo) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.prediction_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(prediction) = rx.recv().await {
                callback(&prediction);
            }
        });

        CallbackHandle::new(callback_id, move || {
            handle.abort();
        })
    }

    // === Food Safety ===

    /// Configure food safety monitoring with a product type (simplified mode).
    ///
    /// This uses simplified mode which applies predefined USDA temperature thresholds
    /// based on the product type.
    pub async fn configure_food_safe(&self, product: FoodSafeProduct) -> Result<()> {
        let config = product.to_config(Serving::ServedImmediately);
        self.configure_food_safe_with_config(config).await
    }

    /// Configure food safety monitoring with a product type and serving mode (simplified mode).
    pub async fn configure_food_safe_with_serving(
        &self,
        product: FoodSafeProduct,
        serving: Serving,
    ) -> Result<()> {
        let config = product.to_config(serving);
        self.configure_food_safe_with_config(config).await
    }

    /// Configure food safety monitoring with full configuration (integrated mode).
    ///
    /// This allows specifying custom parameters for time-temperature integration
    /// including Z-value, D-value, reference temperature, and target log reduction.
    pub async fn configure_food_safe_with_config(&self, config: FoodSafeConfig) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let config_bytes = config.to_bytes();
        let message = build_configure_food_safe_request(&config_bytes);
        self.send_uart_message(&message).await?;

        self.state.write().food_safe_data = Some(FoodSafeData::with_config(config));

        Ok(())
    }

    /// Configure food safety monitoring with integrated mode for a product.
    ///
    /// Integrated mode calculates log reduction based on time-temperature integration.
    pub async fn configure_food_safe_integrated(
        &self,
        product: FoodSafeProduct,
        serving: Serving,
    ) -> Result<()> {
        let config = product.to_integrated_config(serving);
        self.configure_food_safe_with_config(config).await
    }

    /// Reset food safety calculations.
    pub async fn reset_food_safe(&self) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_reset_food_safe_request();
        self.send_uart_message(&message).await?;

        self.state.write().food_safe_data = None;

        Ok(())
    }

    /// Get current food safety data.
    pub fn food_safe_data(&self) -> Option<FoodSafeData> {
        self.state.read().food_safe_data.clone()
    }

    // === Battery & Status ===

    /// Get current battery status.
    pub fn battery_status(&self) -> BatteryStatus {
        self.state.read().battery_status
    }

    /// Get overheating information.
    pub fn overheating(&self) -> Overheating {
        self.state.read().overheating
    }

    /// Get current operational mode.
    pub fn mode(&self) -> ProbeMode {
        self.state.read().mode
    }

    // === Power Mode & Preferences ===

    /// Get current power mode.
    ///
    /// Returns `None` if the probe hasn't sent thermometer preferences yet.
    pub fn power_mode(&self) -> Option<PowerMode> {
        self.state
            .read()
            .thermometer_preferences
            .map(|p| p.power_mode)
    }

    /// Check if the probe is in always-on mode.
    pub fn is_always_on(&self) -> bool {
        self.state
            .read()
            .thermometer_preferences
            .map(|p| p.is_always_on())
            .unwrap_or(false)
    }

    /// Get thermometer preferences.
    pub fn thermometer_preferences(&self) -> Option<ThermometerPreferences> {
        self.state.read().thermometer_preferences
    }

    /// Set the power mode.
    ///
    /// - `PowerMode::Normal`: Probe will auto power-off when placed in charger.
    /// - `PowerMode::AlwaysOn`: Probe stays powered even in charger.
    pub async fn set_power_mode(&self, mode: PowerMode) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_set_power_mode_request(mode.to_raw());
        self.send_uart_message(&message).await?;

        // Update local state
        let mut state = self.state.write();
        state.thermometer_preferences = Some(ThermometerPreferences::with_power_mode(mode));

        Ok(())
    }

    /// Reset the thermometer to factory defaults.
    ///
    /// This will reset all settings including probe ID, color, alarms, etc.
    pub async fn reset_thermometer(&self) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_reset_thermometer_request();
        self.send_uart_message(&message).await
    }

    // === Temperature Alarms ===

    /// Get current alarm configuration.
    ///
    /// Returns `None` if the probe hasn't sent alarm status yet.
    pub fn alarm_config(&self) -> Option<AlarmConfig> {
        self.state.read().alarm_config.clone()
    }

    /// Check if any alarm is currently triggered.
    pub fn any_alarm_tripped(&self) -> bool {
        self.state
            .read()
            .alarm_config
            .as_ref()
            .map(|c| c.any_tripped())
            .unwrap_or(false)
    }

    /// Check if any alarm is currently sounding.
    pub fn any_alarm_alarming(&self) -> bool {
        self.state
            .read()
            .alarm_config
            .as_ref()
            .map(|c| c.any_alarming())
            .unwrap_or(false)
    }

    /// Check if any alarm is enabled.
    pub fn any_alarm_enabled(&self) -> bool {
        self.state
            .read()
            .alarm_config
            .as_ref()
            .map(|c| c.any_enabled())
            .unwrap_or(false)
    }

    /// Set temperature alarms.
    ///
    /// Configures high and low temperature alarms for all sensors (T1-T8) and
    /// virtual sensors (Core, Surface, Ambient).
    ///
    /// See `AlarmConfig` for how to configure individual alarms.
    pub async fn set_alarms(&self, config: &AlarmConfig) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let config_bytes = config.to_bytes();
        let message = build_set_high_low_alarms_request(&config_bytes);
        self.send_uart_message(&message).await?;

        // Update local state
        self.state.write().alarm_config = Some(config.clone());

        Ok(())
    }

    /// Silence any currently sounding alarms.
    pub async fn silence_alarms(&self) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_silence_alarms_request();
        self.send_uart_message(&message).await
    }

    /// Set a high temperature alarm for the core (virtual) sensor.
    ///
    /// This is a convenience method that creates an alarm config with just
    /// the core high alarm set.
    pub async fn set_core_high_alarm(&self, temperature_celsius: f64) -> Result<()> {
        let mut config = self.alarm_config().unwrap_or_default();
        config.set_core_high_alarm(temperature_celsius, true);
        self.set_alarms(&config).await
    }

    /// Set a low temperature alarm for the core (virtual) sensor.
    ///
    /// This is a convenience method that creates an alarm config with just
    /// the core low alarm set.
    pub async fn set_core_low_alarm(&self, temperature_celsius: f64) -> Result<()> {
        let mut config = self.alarm_config().unwrap_or_default();
        config.set_core_low_alarm(temperature_celsius, true);
        self.set_alarms(&config).await
    }

    /// Disable all alarms.
    pub async fn disable_all_alarms(&self) -> Result<()> {
        let config = AlarmConfig::new();
        self.set_alarms(&config).await
    }

    // === Configuration ===

    /// Set probe ID (1-8).
    pub async fn set_id(&self, id: ProbeId) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_set_probe_id_request(id.as_u8());
        self.send_uart_message(&message).await?;

        let mut state = self.state.write();
        state.probe_id = id;
        state.probe_id_set_at = Some(Instant::now());

        Ok(())
    }

    /// Set probe color.
    pub async fn set_color(&self, color: ProbeColor) -> Result<()> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_set_probe_color_request(color.to_raw());
        self.send_uart_message(&message).await?;

        let mut state = self.state.write();
        state.color = color;
        state.color_set_at = Some(Instant::now());

        Ok(())
    }

    /// Read session information.
    pub async fn read_session_info(&self) -> Result<SessionInfo> {
        if !self.connection.is_connected() {
            return Err(Error::NotConnected);
        }

        let message = build_read_session_info_request();
        self.send_uart_message(&message).await?;

        // In a real implementation, we'd wait for the response
        // For now, return cached or default
        Ok(self.state.read().session_info.clone().unwrap_or_default())
    }

    // === Firmware ===

    /// Read firmware version.
    pub async fn read_firmware_version(&self) -> Result<String> {
        let _handler = self
            .characteristics
            .read()
            .as_ref()
            .ok_or(Error::NotConnected)?;

        // This won't work because we can't clone CharacteristicHandler
        // We need a different approach
        Err(Error::NotSupported {
            operation: "read_firmware_version requires connected state".to_string(),
        })
    }

    /// Read hardware revision.
    pub async fn read_hardware_revision(&self) -> Result<String> {
        Err(Error::NotSupported {
            operation: "read_hardware_revision requires connected state".to_string(),
        })
    }

    // === Internal ===

    /// Send a UART message.
    async fn send_uart_message(&self, message: &UartMessage) -> Result<()> {
        let handler_guard = self.characteristics.read();
        let handler = handler_guard.as_ref().ok_or(Error::NotConnected)?;

        let data = message.to_bytes();
        handler.write(&UART_RX_UUID, &data, false).await
    }
}

impl std::fmt::Debug for Probe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Probe")
            .field("identifier", &self.identifier)
            .field("serial_number", &self.serial_number_string())
            .field("connection_state", &self.connection_state())
            .finish()
    }
}
