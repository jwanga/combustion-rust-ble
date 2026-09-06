//! Device manager for discovering and managing Combustion Predictive Probes.
//!
//! This module handles BLE scanning and probe lifecycle management.
//! Only Predictive Probes (ProductType::PredictiveProbe) are discovered
//! and managed. Other Combustion devices (Display, Booster, MeatNet Repeater,
//! Giant Grill Gauge) are intentionally filtered out.

use btleplug::platform::{Adapter, PeripheralId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Semaphore};
use tracing::{debug, info, warn};

use crate::ble::scanner::{BleScanner, ProbeDiscoveryEvent, ScanMode};
use crate::error::Result;
use crate::probe::{CallbackHandle, Probe};

/// Maximum number of probes that can be managed simultaneously.
pub const MAX_PROBES: usize = 8;

/// A probe-discovery broadcast payload.
///
/// Carries the latest RSSI reading from the advertising packet that triggered the
/// discovery event, so consumers can log signal strength without having to read it back
/// from the probe's cached state (which is updated in the same tick but is otherwise
/// rate-dependent).
#[derive(Debug, Clone)]
pub struct DiscoveredProbeEvent {
    /// The probe that was discovered or updated.
    pub probe: Arc<Probe>,
    /// RSSI from the advertising packet, in dBm, if reported by the OS.
    pub rssi: Option<i16>,
}

/// A probe-disconnect broadcast payload.
///
/// Parallels [`DiscoveredProbeEvent`]. Currently carries only the probe; the struct
/// shape exists so future additions (last-known RSSI, disconnect reason) don't force
/// another breaking change to the broadcast type.
#[derive(Debug, Clone)]
pub struct DisconnectedProbeEvent {
    /// The probe that disconnected at the link layer.
    pub probe: Arc<Probe>,
}

/// Central manager for discovering and managing Combustion probes.
pub struct DeviceManager {
    /// BLE scanner.
    scanner: Arc<BleScanner>,
    /// Discovered probes by serial number (as hex string).
    probes: Arc<RwLock<HashMap<String, Arc<Probe>>>>,
    /// Whether MeatNet is enabled.
    meatnet_enabled: AtomicBool,
    /// Probe discovery channel.
    probe_discovered_tx: broadcast::Sender<DiscoveredProbeEvent>,
    /// Probe stale channel.
    probe_stale_tx: broadcast::Sender<Arc<Probe>>,
    /// Probe link-layer-disconnect channel — fires when the platform tells us a probe
    /// went offline (`CentralEvent::DeviceDisconnected`).
    probe_disconnected_tx: broadcast::Sender<DisconnectedProbeEvent>,
    /// Adapter-level semaphore (permits=1) shared with every `Probe`'s
    /// `ConnectionManager` so that BlueZ `Connect()` calls are serialized. See
    /// `ConnectionManager::connect_permit` for the rationale.
    connect_permit: Arc<Semaphore>,
    /// Callback ID counter.
    callback_counter: AtomicU64,
    /// Background task handle.
    background_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    /// Running flag.
    is_running: Arc<AtomicBool>,
}

impl DeviceManager {
    /// Create a new DeviceManager instance.
    ///
    /// Opens the platform's btleplug [`Manager`](btleplug::platform::Manager) and uses
    /// the first adapter it reports. If the application already owns an
    /// [`Adapter`], use [`DeviceManager::with_adapter`] instead so both share one
    /// handle.
    ///
    /// # Errors
    ///
    /// Returns an error if Bluetooth is not available.
    pub async fn new() -> Result<Self> {
        let scanner = BleScanner::new().await?;
        Ok(Self::from_scanner(scanner))
    }

    /// Create a DeviceManager on an adapter the application already holds.
    ///
    /// This is the entry point for applications that talk to other BLE devices
    /// through btleplug and do not want this library to open a second
    /// [`Manager`](btleplug::platform::Manager). The `Adapter` type must come from
    /// the same btleplug version this crate links against; use the re-exported
    /// [`combustion_rust_ble::btleplug`](crate::btleplug) to guarantee that.
    ///
    /// The manager does not touch the adapter until
    /// [`start_scanning`](Self::start_scanning) is called.
    ///
    /// # Shared scan state
    ///
    /// Scan state belongs to the adapter, not to this manager. `start_scanning` calls
    /// `Adapter::start_scan` with an empty `ScanFilter` (replacing any filter the
    /// application set), and `stop_scanning` / `shutdown` call `Adapter::stop_scan`,
    /// which also ends any scan the application started on the same adapter. On BlueZ,
    /// calling `start_scanning` while the application is already scanning returns
    /// `Error::Bluetooth` (`InProgress`). Coordinate scanning through one owner: either
    /// let this manager drive the scan and read other peripherals via
    /// [`adapter()`](Self::adapter), or stop the application's scan before calling
    /// `start_scanning`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use combustion_rust_ble::btleplug::api::Manager as _;
    /// use combustion_rust_ble::btleplug::platform::Manager;
    /// use combustion_rust_ble::{DeviceManager, Error, Result};
    ///
    /// # async fn run() -> Result<()> {
    /// let btle = Manager::new().await?;
    /// let adapter = btle
    ///     .adapters()
    ///     .await?
    ///     .into_iter()
    ///     .next()
    ///     .ok_or(Error::BluetoothUnavailable)?;
    ///
    /// let manager = DeviceManager::with_adapter(adapter);
    /// manager.start_scanning().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_adapter(adapter: Adapter) -> Self {
        Self::from_scanner(BleScanner::with_adapter(adapter))
    }

    /// Shared construction path for [`new`](Self::new) and
    /// [`with_adapter`](Self::with_adapter).
    fn from_scanner(scanner: BleScanner) -> Self {
        let (probe_discovered_tx, _) = broadcast::channel(32);
        let (probe_stale_tx, _) = broadcast::channel(32);
        let (probe_disconnected_tx, _) = broadcast::channel(32);

        Self {
            scanner: Arc::new(scanner),
            probes: Arc::new(RwLock::new(HashMap::new())),
            meatnet_enabled: AtomicBool::new(false),
            probe_discovered_tx,
            probe_stale_tx,
            probe_disconnected_tx,
            connect_permit: Arc::new(Semaphore::new(1)),
            callback_counter: AtomicU64::new(0),
            background_handle: RwLock::new(None),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the btleplug [`Adapter`] this manager scans and connects through.
    pub fn adapter(&self) -> &Adapter {
        self.scanner.adapter()
    }

    /// Start scanning for probes, owning the adapter scan.
    ///
    /// Calls `Adapter::start_scan`; [`stop_scanning`](Self::stop_scanning) and
    /// [`shutdown`](Self::shutdown) will call `Adapter::stop_scan`. If the host
    /// application already runs a scan on this adapter, use [`attach`](Self::attach).
    pub async fn start_scanning(&self) -> Result<()> {
        self.start_with(ScanMode::Owned).await
    }

    /// Attach to a scan the host application already started on this adapter.
    ///
    /// Processes discovery events **without** calling `Adapter::start_scan`. A later
    /// [`stop_scanning`](Self::stop_scanning) or [`shutdown`](Self::shutdown) stops
    /// event processing but never calls `Adapter::stop_scan`, so the host's scan keeps
    /// running. See [`ScanMode`] and [`scan_mode`](Self::scan_mode).
    ///
    /// Returns [`Error::ScanModeMismatch`](crate::Error::ScanModeMismatch) if this
    /// manager already owns a scan started by [`start_scanning`](Self::start_scanning);
    /// the reverse also holds.
    pub async fn attach(&self) -> Result<()> {
        self.start_with(ScanMode::Attached).await
    }

    async fn start_with(&self, mode: ScanMode) -> Result<()> {
        if self.is_running.load(Ordering::SeqCst) {
            debug!("Already scanning");
            return Ok(());
        }

        match mode {
            ScanMode::Owned => {
                info!("Starting device manager scanning");
                self.scanner.start_scanning().await?;
            }
            ScanMode::Attached => {
                info!("Attaching device manager to host-owned scan");
                self.scanner.attach().await?;
            }
        }

        self.spawn_event_loop();
        Ok(())
    }

    /// How the current scan was started, or `None` when not scanning.
    pub fn scan_mode(&self) -> Option<ScanMode> {
        self.scanner.scan_mode()
    }

    /// Spawn the background task that turns scanner events into probe updates.
    fn spawn_event_loop(&self) {
        if let Some(stale) = self.background_handle.write().take() {
            // Only reachable if a previous stop failed mid-way; never run two loops.
            stale.abort();
        }
        self.is_running.store(true, Ordering::SeqCst);

        // Start background task to process discovery events
        let scanner = self.scanner.clone();
        let probes = self.probes.clone();
        let probe_discovered_tx = self.probe_discovered_tx.clone();
        let probe_stale_tx = self.probe_stale_tx.clone();
        let probe_disconnected_tx = self.probe_disconnected_tx.clone();
        let connect_permit = self.connect_permit.clone();
        let is_running = self.is_running.clone();

        let handle = tokio::spawn(async move {
            let mut rx = scanner.subscribe();
            let mut disconnect_rx = scanner.subscribe_disconnects();

            while is_running.load(Ordering::SeqCst) {
                tokio::select! {
                    Ok(event) = rx.recv() => {
                        Self::handle_discovery_event(
                            event,
                            &probes,
                            &probe_discovered_tx,
                            &connect_permit,
                        ).await;
                    }
                    Ok(peripheral_id) = disconnect_rx.recv() => {
                        Self::handle_disconnect_event(
                            peripheral_id,
                            &probes,
                            &probe_disconnected_tx,
                        ).await;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        // Check for stale probes
                        Self::check_stale_probes(&probes, &probe_stale_tx);
                    }
                }
            }

            debug!("Device manager background task ended");
        });

        *self.background_handle.write() = Some(handle);
    }

    /// Stop processing scan events.
    ///
    /// Calls `Adapter::stop_scan` only when this manager owns the scan
    /// ([`ScanMode::Owned`]); after [`attach`](Self::attach) the host's scan is left
    /// running.
    pub async fn stop_scanning(&self) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Stopping device manager scanning");

        // Stop the scanner first: if `stop_scan` fails the scanner stays active and so
        // do we, leaving a consistent state the caller can retry from.
        self.scanner.stop_scanning().await?;
        self.is_running.store(false, Ordering::SeqCst);

        // Wait for background task
        let previous = self.background_handle.write().take();
        if let Some(handle) = previous {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Get all discovered probes.
    pub fn probes(&self) -> HashMap<String, Arc<Probe>> {
        self.probes.read().clone()
    }

    /// Get a specific probe by serial number (as hex string, e.g., "100120BA").
    pub fn get_probe(&self, serial_number: &str) -> Option<Arc<Probe>> {
        self.probes.read().get(serial_number).cloned()
    }

    /// Get the nearest probe by signal strength.
    pub fn get_nearest_probe(&self) -> Option<Arc<Probe>> {
        self.probes
            .read()
            .values()
            .filter(|p| !p.is_stale())
            .max_by_key(|p| p.rssi().unwrap_or(i16::MIN))
            .cloned()
    }

    /// Get probes sorted by signal strength (strongest first).
    pub fn get_probes_by_signal(&self) -> Vec<Arc<Probe>> {
        let mut probes: Vec<_> = self
            .probes
            .read()
            .values()
            .filter(|p| !p.is_stale())
            .cloned()
            .collect();

        probes.sort_by_key(|p| std::cmp::Reverse(p.rssi().unwrap_or(i16::MIN)));
        probes
    }

    /// Subscribe to probe discovery events.
    ///
    /// Each event carries the probe and the RSSI from the advertising packet that
    /// triggered the discovery.
    pub fn subscribe_probe_discovered(&self) -> broadcast::Receiver<DiscoveredProbeEvent> {
        self.probe_discovered_tx.subscribe()
    }

    /// Register a callback for when probes are discovered/updated.
    pub fn on_probe_discovered<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(DiscoveredProbeEvent) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.probe_discovered_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                callback(event);
            }
        });

        CallbackHandle::new(callback_id, move || {
            handle.abort();
        })
    }

    /// Subscribe to probe stale events.
    pub fn subscribe_probe_stale(&self) -> broadcast::Receiver<Arc<Probe>> {
        self.probe_stale_tx.subscribe()
    }

    /// Subscribe to probe link-layer disconnect events.
    ///
    /// Fires when the platform (BlueZ / Core Bluetooth / etc.) reports that a known
    /// probe went offline. By the time subscribers receive the event, the probe's
    /// internal [`ConnectionState`](crate::ble::connection::ConnectionState) has
    /// already been reset to `Disconnected` and its cached GATT handles cleared.
    /// Callers can drive their own reconnect policy from here.
    pub fn subscribe_probe_disconnected(&self) -> broadcast::Receiver<DisconnectedProbeEvent> {
        self.probe_disconnected_tx.subscribe()
    }

    /// Register a callback for when probes disconnect at the link layer.
    pub fn on_probe_disconnected<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(DisconnectedProbeEvent) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.probe_disconnected_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                callback(event);
            }
        });

        CallbackHandle::new(callback_id, move || {
            handle.abort();
        })
    }

    /// Register a callback for when probes become stale/disconnected.
    pub fn on_probe_stale<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(Arc<Probe>) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.probe_stale_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(probe) = rx.recv().await {
                callback(probe);
            }
        });

        CallbackHandle::new(callback_id, move || {
            handle.abort();
        })
    }

    /// Enable MeatNet support for Display/Booster nodes.
    pub fn enable_meatnet(&self) {
        self.meatnet_enabled.store(true, Ordering::SeqCst);
        info!("MeatNet support enabled");
    }

    /// Disable MeatNet support.
    pub fn disable_meatnet(&self) {
        self.meatnet_enabled.store(false, Ordering::SeqCst);
        info!("MeatNet support disabled");
    }

    /// Check if MeatNet is enabled.
    pub fn is_meatnet_enabled(&self) -> bool {
        self.meatnet_enabled.load(Ordering::SeqCst)
    }

    /// Clean shutdown of all connections and scanning.
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down device manager");

        // Stop scanning
        self.stop_scanning().await?;

        // Disconnect all probes
        let probes: Vec<_> = self.probes.read().values().cloned().collect();
        for probe in probes {
            if let Err(e) = probe.disconnect().await {
                warn!("Error disconnecting probe {}: {}", probe.identifier(), e);
            }
        }

        // Clear probes
        self.probes.write().clear();

        Ok(())
    }

    /// Get the number of discovered probes.
    pub fn probe_count(&self) -> usize {
        self.probes.read().len()
    }

    /// Check if scanning is active.
    pub fn is_scanning(&self) -> bool {
        self.scanner.is_scanning()
    }

    /// Handle a discovery event from the scanner.
    ///
    /// Only Predictive Probes (ProductType::PredictiveProbe) are added to the probe list.
    /// Other Combustion devices (Display, Booster, MeatNet Repeater, etc.) are ignored.
    async fn handle_discovery_event(
        event: ProbeDiscoveryEvent,
        probes: &Arc<RwLock<HashMap<String, Arc<Probe>>>>,
        probe_discovered_tx: &broadcast::Sender<DiscoveredProbeEvent>,
        connect_permit: &Arc<Semaphore>,
    ) {
        let advertising_data = match &event.advertising_data {
            Some(data) => data,
            None => return, // Not a Combustion device with parseable data
        };

        // Only accept Predictive Probes - ignore Display, Booster, MeatNet Repeater, etc.
        if !advertising_data.product_type.is_predictive_probe() {
            debug!(
                "Ignoring non-probe device: {:?} (serial: {:08X})",
                advertising_data.product_type, advertising_data.serial_number
            );
            return;
        }

        let ble_identifier = event.identifier.clone();
        let serial_number = advertising_data.serial_number;

        // Use serial number as the unique key to avoid duplicates from different BLE identifiers
        // On macOS, the same physical probe can sometimes be discovered with different UUIDs
        let serial_key = format!("{:08X}", serial_number);

        // Check if we already know this probe by serial number
        let existing = probes.read().get(&serial_key).cloned();

        let probe = match existing {
            Some(probe) => {
                // Update existing probe with new data
                probe.update_from_advertising(advertising_data, event.rssi);
                probe
            }
            None => {
                // Check if we've hit the limit
                if probes.read().len() >= MAX_PROBES {
                    warn!(
                        "Maximum probe count ({}) reached, ignoring new probe",
                        MAX_PROBES
                    );
                    return;
                }

                // Create new probe — all probes on this adapter share one
                // connect_permit so BlueZ Connect() calls are serialized.
                let probe = Arc::new(Probe::new(
                    ble_identifier.clone(),
                    event.peripheral,
                    serial_number,
                    connect_permit.clone(),
                ));
                probe.update_from_advertising(advertising_data, event.rssi);

                info!(
                    "Discovered new probe: {} (BLE: {})",
                    probe.serial_number_string(),
                    ble_identifier
                );

                probes.write().insert(serial_key, probe.clone());
                probe
            }
        };

        // Send discovery event
        let _ = probe_discovered_tx.send(DiscoveredProbeEvent {
            probe,
            rssi: event.rssi,
        });
    }

    /// Check for stale probes and emit events.
    fn check_stale_probes(
        probes: &Arc<RwLock<HashMap<String, Arc<Probe>>>>,
        probe_stale_tx: &broadcast::Sender<Arc<Probe>>,
    ) {
        for probe in probes.read().values() {
            if probe.is_stale() {
                let _ = probe_stale_tx.send(probe.clone());
            }
        }
    }

    /// Route a `DeviceDisconnected` event from the scanner to the affected probe.
    ///
    /// Looks up the probe by its BLE identifier (the string form of
    /// [`PeripheralId`], which is what we store on the [`Probe`] at discovery time).
    /// The probe registry caps at [`MAX_PROBES`] entries so the linear scan is
    /// trivial. Returns silently if the peripheral is not one of ours.
    async fn handle_disconnect_event(
        peripheral_id: PeripheralId,
        probes: &Arc<RwLock<HashMap<String, Arc<Probe>>>>,
        probe_disconnected_tx: &broadcast::Sender<DisconnectedProbeEvent>,
    ) {
        let identifier_str = peripheral_id.to_string();

        // Collect the matching probe into a local, releasing the lock before awaiting.
        let probe = {
            let probes_read = probes.read();
            probes_read
                .values()
                .find(|p| p.identifier() == identifier_str)
                .cloned()
        };

        let Some(probe) = probe else {
            debug!(
                peripheral_id = %identifier_str,
                "DeviceDisconnected for peripheral not in our probe registry — ignoring",
            );
            return;
        };

        info!(
            serial = %probe.serial_number_string(),
            peripheral_id = %identifier_str,
            "Routing link-layer disconnect to probe",
        );

        probe.handle_link_disconnect().await;

        // Broadcast to external subscribers (e.g. caller-side reconnect drivers).
        // Send-error means no live subscribers, which is fine.
        let _ = probe_disconnected_tx.send(DisconnectedProbeEvent { probe });
    }
}

impl Drop for DeviceManager {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_probes_constant() {
        assert_eq!(MAX_PROBES, 8);
    }

    /// `with_adapter` must stay a plain, infallible, non-async constructor so callers can
    /// wrap an adapter they already hold without an extra await or error path.
    #[test]
    fn test_with_adapter_signature() {
        let _ctor: fn(Adapter) -> DeviceManager = DeviceManager::with_adapter;
        let _getter: fn(&DeviceManager) -> &Adapter = DeviceManager::adapter;
    }

    /// LIB-7: the new `probe_disconnected_tx` channel must accept sends even when there
    /// are no live subscribers (send returns `Err(SendError)`, which we tolerate). Two
    /// subscribers should both receive a broadcasted event.
    #[tokio::test]
    async fn test_probe_disconnected_broadcast_plumbing() {
        let (tx, mut rx_a) = broadcast::channel::<String>(8);
        let mut rx_b = tx.subscribe();

        // No live subs: cannot happen here since we created two, but exercise the
        // pattern used by handle_disconnect_event — send-error is ignored.
        let _ = tx.send("c2:71:2a:e7:88:d0".to_string());

        let a = rx_a.recv().await.expect("subscriber A receives event");
        let b = rx_b.recv().await.expect("subscriber B receives event");
        assert_eq!(a, "c2:71:2a:e7:88:d0");
        assert_eq!(b, "c2:71:2a:e7:88:d0");
    }

    /// LIB-7: with no subscribers, broadcasting a disconnect must not panic.
    /// `handle_disconnect_event` calls `let _ = probe_disconnected_tx.send(probe)` —
    /// this asserts that pattern is safe.
    #[tokio::test]
    async fn test_probe_disconnected_send_with_no_subscribers_is_safe() {
        let (tx, _initial_rx) = broadcast::channel::<u8>(4);
        drop(_initial_rx);
        // No subscribers — send returns Err but we don't unwrap, mirroring production code.
        let result = tx.send(42);
        assert!(result.is_err(), "send with no subscribers returns Err");
    }
}
