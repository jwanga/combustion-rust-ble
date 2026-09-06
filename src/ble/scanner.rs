//! BLE scanning functionality.
//!
//! Provides the scanner for discovering Combustion probes.

use async_trait::async_trait;
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::stream::StreamExt;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace};

use crate::ble::advertising::AdvertisingData;
use crate::ble::uuids::COMBUSTION_MANUFACTURER_ID;
use crate::error::{Error, Result};

/// Event emitted when a probe is discovered or updated.
#[derive(Debug, Clone)]
pub struct ProbeDiscoveryEvent {
    /// The BLE peripheral identifier.
    pub identifier: String,
    /// The peripheral handle.
    pub peripheral: Peripheral,
    /// Parsed advertising data (if available).
    pub advertising_data: Option<AdvertisingData>,
    /// Signal strength in dBm.
    pub rssi: Option<i16>,
}

/// Who owns the adapter-level scan this scanner is consuming.
///
/// Scan state belongs to the btleplug [`Adapter`], not to this crate. When the host
/// application shares its adapter with other BLE drivers, exactly one party should call
/// `start_scan` / `stop_scan`; everyone else only reads the event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// This crate called `Adapter::start_scan` and will call `Adapter::stop_scan` when
    /// scanning stops.
    Owned,
    /// The host already runs the scan. This crate only processes events and never
    /// calls `stop_scan`.
    Attached,
}

/// The adapter-level scan calls, abstracted so ownership logic can be unit-tested
/// without Bluetooth hardware.
#[async_trait]
pub trait ScanControl: Send + Sync {
    /// Start the adapter scan with the given filter.
    async fn start_scan(&self, filter: ScanFilter) -> btleplug::Result<()>;
    /// Stop the adapter scan.
    async fn stop_scan(&self) -> btleplug::Result<()>;
}

#[async_trait]
impl ScanControl for Adapter {
    async fn start_scan(&self, filter: ScanFilter) -> btleplug::Result<()> {
        Central::start_scan(self, filter).await
    }

    async fn stop_scan(&self) -> btleplug::Result<()> {
        Central::stop_scan(self).await
    }
}

/// Owner-vs-attached scan state machine.
///
/// Kept separate from [`BleScanner`] so the "never call `stop_scan` in attached
/// mode" rule can be tested against a fake [`ScanControl`].
pub(crate) struct ScanSession {
    /// Whether the event loop should keep running. Shared with the spawned task.
    active: Arc<RwLock<bool>>,
    /// How the current scan was started; `None` while inactive.
    mode: RwLock<Option<ScanMode>>,
}

impl ScanSession {
    fn new() -> Self {
        Self {
            active: Arc::new(RwLock::new(false)),
            mode: RwLock::new(None),
        }
    }

    /// The loop flag shared with the event-processing task.
    fn active_flag(&self) -> Arc<RwLock<bool>> {
        self.active.clone()
    }

    fn is_active(&self) -> bool {
        *self.active.read()
    }

    fn mode(&self) -> Option<ScanMode> {
        *self.mode.read()
    }

    /// Activate the session. In [`ScanMode::Owned`] this starts the adapter scan; in
    /// [`ScanMode::Attached`] the adapter is not touched. Returns `Ok(false)` if the
    /// session was already active (no adapter call is made).
    async fn begin(
        &self,
        control: &dyn ScanControl,
        mode: ScanMode,
        filter: ScanFilter,
    ) -> Result<bool> {
        if self.is_active() {
            return Ok(false);
        }
        if mode == ScanMode::Owned {
            control.start_scan(filter).await.map_err(Error::Bluetooth)?;
        }
        *self.mode.write() = Some(mode);
        *self.active.write() = true;
        Ok(true)
    }

    /// Deactivate the session. Calls `stop_scan` only if this session owns the scan.
    /// Returns `Ok(false)` if the session was not active.
    async fn end(&self, control: &dyn ScanControl) -> Result<bool> {
        if !self.is_active() {
            return Ok(false);
        }
        *self.active.write() = false;
        let mode = self.mode.write().take();
        if mode == Some(ScanMode::Owned) {
            control.stop_scan().await.map_err(Error::Bluetooth)?;
        }
        Ok(true)
    }
}

/// BLE scanner for discovering Combustion probes.
pub struct BleScanner {
    /// The BLE adapter to use for scanning.
    adapter: Adapter,
    /// Scan ownership state.
    session: ScanSession,
    /// Discovered peripherals.
    discovered: Arc<RwLock<HashMap<String, ProbeDiscoveryEvent>>>,
    /// Channel for discovery events.
    event_tx: broadcast::Sender<ProbeDiscoveryEvent>,
    /// Channel for link-layer disconnect events (`CentralEvent::DeviceDisconnected`).
    /// Carries the `PeripheralId` so subscribers can look up the affected probe.
    disconnect_tx: broadcast::Sender<PeripheralId>,
    /// Handle to the scanning task.
    scan_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl BleScanner {
    /// Create a new BLE scanner.
    ///
    /// # Errors
    ///
    /// Returns an error if Bluetooth is not available.
    pub async fn new() -> Result<Self> {
        let manager = Manager::new()
            .await
            .map_err(|_e| Error::BluetoothUnavailable)?;

        let adapters = manager.adapters().await.map_err(Error::Bluetooth)?;

        let adapter = adapters
            .into_iter()
            .next()
            .ok_or(Error::BluetoothUnavailable)?;

        info!(
            "Using Bluetooth adapter: {:?}",
            adapter.adapter_info().await.ok()
        );

        Ok(Self::with_adapter(adapter))
    }

    /// Create a new BLE scanner with a specific adapter.
    pub fn with_adapter(adapter: Adapter) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (disconnect_tx, _) = broadcast::channel(32);

        Self {
            adapter,
            session: ScanSession::new(),
            discovered: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            disconnect_tx,
            scan_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Start scanning for probes, owning the adapter scan.
    ///
    /// Calls `Adapter::start_scan` with an empty [`ScanFilter`] and, later,
    /// `Adapter::stop_scan` from [`stop_scanning`](Self::stop_scanning). Use
    /// [`attach`](Self::attach) instead when the host already runs the scan.
    ///
    /// # Errors
    ///
    /// Returns an error if scanning cannot be started.
    pub async fn start_scanning(&self) -> Result<()> {
        self.start_with(ScanMode::Owned, ScanFilter::default())
            .await
    }

    /// Attach to a scan the host application already started on this adapter.
    ///
    /// Starts the event-processing task **without** calling `Adapter::start_scan`.
    /// A later [`stop_scanning`](Self::stop_scanning) stops the task but never calls
    /// `Adapter::stop_scan`, so the host's scan keeps running.
    ///
    /// # Errors
    ///
    /// Currently infallible in practice; returns `Result` for symmetry with
    /// [`start_scanning`](Self::start_scanning).
    pub async fn attach(&self) -> Result<()> {
        self.start_with(ScanMode::Attached, ScanFilter::default())
            .await
    }

    async fn start_with(&self, mode: ScanMode, filter: ScanFilter) -> Result<()> {
        if !self.session.begin(&self.adapter, mode, filter).await? {
            debug!("Already scanning, ignoring start request");
            return Ok(());
        }

        match mode {
            ScanMode::Owned => info!("Starting BLE scan for Combustion probes"),
            ScanMode::Attached => info!("Attaching to host-owned BLE scan"),
        }

        // Start the event processing task
        let adapter = self.adapter.clone();
        let is_scanning = self.session.active_flag();
        let discovered = self.discovered.clone();
        let event_tx = self.event_tx.clone();
        let disconnect_tx = self.disconnect_tx.clone();

        let handle = tokio::spawn(async move {
            let mut events = match adapter.events().await {
                Ok(events) => events,
                Err(e) => {
                    error!("Failed to get adapter events: {}", e);
                    return;
                }
            };

            while *is_scanning.read() {
                tokio::select! {
                    Some(event) = events.next() => {
                        Self::handle_event(
                            event,
                            &adapter,
                            &discovered,
                            &event_tx,
                            &disconnect_tx,
                        ).await;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        // Check if we should stop scanning
                        if !*is_scanning.read() {
                            break;
                        }
                    }
                }
            }

            debug!("Scan event loop ended");
        });

        *self.scan_handle.write() = Some(handle);

        Ok(())
    }

    /// Stop processing scan events.
    ///
    /// Calls `Adapter::stop_scan` only if this scanner started the scan
    /// ([`ScanMode::Owned`]). After [`attach`](Self::attach) the host's scan is left
    /// running.
    pub async fn stop_scanning(&self) -> Result<()> {
        let mode = self.session.mode();
        if !self.session.end(&self.adapter).await? {
            debug!("Not scanning, ignoring stop request");
            return Ok(());
        }

        match mode {
            Some(ScanMode::Owned) => info!("Stopped BLE scan"),
            _ => info!("Detached from host-owned BLE scan"),
        }

        // Wait for the scan task to complete
        if let Some(handle) = self.scan_handle.write().take() {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Check if currently processing scan events (owned or attached).
    pub fn is_scanning(&self) -> bool {
        self.session.is_active()
    }

    /// How the current scan was started, or `None` when not scanning.
    pub fn scan_mode(&self) -> Option<ScanMode> {
        self.session.mode()
    }

    /// Get all discovered probes.
    pub fn discovered_probes(&self) -> HashMap<String, ProbeDiscoveryEvent> {
        self.discovered.read().clone()
    }

    /// Subscribe to discovery events.
    pub fn subscribe(&self) -> broadcast::Receiver<ProbeDiscoveryEvent> {
        self.event_tx.subscribe()
    }

    /// Subscribe to link-layer disconnect events.
    ///
    /// Each event carries the `PeripheralId` of the device BlueZ (or the platform
    /// equivalent) just told us went offline. Use this to keep higher-level
    /// connection state honest — without this signal, a stale `Connected` state
    /// will short-circuit subsequent reconnect attempts.
    pub fn subscribe_disconnects(&self) -> broadcast::Receiver<PeripheralId> {
        self.disconnect_tx.subscribe()
    }

    /// Get the underlying adapter.
    pub fn adapter(&self) -> &Adapter {
        &self.adapter
    }

    /// Handle a BLE central event.
    async fn handle_event(
        event: btleplug::api::CentralEvent,
        adapter: &Adapter,
        discovered: &Arc<RwLock<HashMap<String, ProbeDiscoveryEvent>>>,
        event_tx: &broadcast::Sender<ProbeDiscoveryEvent>,
        disconnect_tx: &broadcast::Sender<PeripheralId>,
    ) {
        use btleplug::api::CentralEvent;

        match event {
            CentralEvent::DeviceDiscovered(id) => {
                trace!("Device discovered: {:?}", id);
                Self::process_peripheral(adapter, id, discovered, event_tx).await;
            }
            CentralEvent::DeviceUpdated(id) => {
                trace!("Device updated: {:?}", id);
                Self::process_peripheral(adapter, id, discovered, event_tx).await;
            }
            CentralEvent::DeviceConnected(id) => {
                debug!("Device connected: {:?}", id);
            }
            CentralEvent::DeviceDisconnected(id) => {
                debug!("Device disconnected: {:?}", id);
                // Route to subscribers so connection-state trackers can stay honest.
                // No receivers is fine — broadcast::send returns Err in that case and we
                // ignore it.
                let _ = disconnect_tx.send(id);
            }
            CentralEvent::ManufacturerDataAdvertisement {
                id,
                manufacturer_data,
            } => {
                // Check for Combustion manufacturer data
                if manufacturer_data.contains_key(&COMBUSTION_MANUFACTURER_ID) {
                    trace!("Combustion device advertisement: {:?}", id);
                    Self::process_peripheral(adapter, id, discovered, event_tx).await;
                }
            }
            CentralEvent::RssiUpdate { id, rssi } => {
                let key = id.to_string();
                if cfg!(target_os = "linux") {
                    // BlueZ never emits `DeviceUpdated`, so `RssiUpdate` is the only
                    // per-advertisement signal there. Refresh known probes only; the RSSI
                    // is folded into the next `ProbeDiscoveryEvent` via `properties().rssi`.
                    if discovered.read().contains_key(&key) {
                        trace!("RSSI update for {:?}: {} dBm", id, rssi);
                        Self::process_peripheral(adapter, id, discovered, event_tx).await;
                    }
                } else if let Some(entry) = discovered.write().get_mut(&key) {
                    // Windows/macOS already deliver `DeviceUpdated` per advertisement;
                    // avoid a duplicate event and just keep the cached snapshot fresh.
                    entry.rssi = Some(rssi);
                }
            }
            CentralEvent::DeviceServicesModified(id) => {
                // CoreBluetooth only. The GATT table changed; the next connect will
                // rediscover services, so there is nothing to do here.
                trace!("Device services modified: {:?}", id);
            }
            CentralEvent::ServiceDataAdvertisement { .. } => {}
            CentralEvent::ServicesAdvertisement { .. } => {}
            CentralEvent::StateUpdate(_) => {}
        }
    }

    /// Process a discovered peripheral.
    async fn process_peripheral(
        adapter: &Adapter,
        id: btleplug::platform::PeripheralId,
        discovered: &Arc<RwLock<HashMap<String, ProbeDiscoveryEvent>>>,
        event_tx: &broadcast::Sender<ProbeDiscoveryEvent>,
    ) {
        let peripheral = match adapter.peripheral(&id).await {
            Ok(p) => p,
            Err(e) => {
                trace!("Failed to get peripheral: {}", e);
                return;
            }
        };

        let properties = match peripheral.properties().await {
            Ok(Some(p)) => p,
            _ => return,
        };

        // Check for Combustion manufacturer data
        let advertising_data = properties
            .manufacturer_data
            .get(&COMBUSTION_MANUFACTURER_ID)
            .and_then(|data| AdvertisingData::parse(data).ok());

        // Only process Combustion probes
        let is_combustion = advertising_data.is_some()
            || properties
                .local_name
                .as_ref()
                .map(|n| n.contains("Combustion"))
                .unwrap_or(false);

        if !is_combustion {
            return;
        }

        let identifier = id.to_string();

        let event = ProbeDiscoveryEvent {
            identifier: identifier.clone(),
            peripheral,
            advertising_data,
            rssi: properties.rssi,
        };

        // Update discovered map
        discovered.write().insert(identifier, event.clone());

        // Send event
        let _ = event_tx.send(event);
    }
}

impl Drop for BleScanner {
    fn drop(&mut self) {
        *self.session.active.write() = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counting fake for the adapter's scan calls.
    #[derive(Default)]
    struct FakeScan {
        starts: AtomicUsize,
        stops: AtomicUsize,
    }

    #[async_trait]
    impl ScanControl for FakeScan {
        async fn start_scan(&self, _filter: ScanFilter) -> btleplug::Result<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop_scan(&self) -> btleplug::Result<()> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn attached_session_never_calls_start_or_stop_scan() {
        let fake = FakeScan::default();
        let session = ScanSession::new();

        assert!(session
            .begin(&fake, ScanMode::Attached, ScanFilter::default())
            .await
            .unwrap());
        assert_eq!(session.mode(), Some(ScanMode::Attached));
        assert!(session.is_active());

        assert!(session.end(&fake).await.unwrap());
        assert!(!session.is_active());
        assert_eq!(session.mode(), None);

        assert_eq!(
            fake.starts.load(Ordering::SeqCst),
            0,
            "attach must not start_scan"
        );
        assert_eq!(
            fake.stops.load(Ordering::SeqCst),
            0,
            "detach must not stop_scan"
        );
    }

    #[tokio::test]
    async fn owned_session_starts_and_stops_adapter_scan_once() {
        let fake = FakeScan::default();
        let session = ScanSession::new();

        assert!(session
            .begin(&fake, ScanMode::Owned, ScanFilter::default())
            .await
            .unwrap());
        // Second begin is a no-op and does not touch the adapter again.
        assert!(!session
            .begin(&fake, ScanMode::Attached, ScanFilter::default())
            .await
            .unwrap());
        assert_eq!(session.mode(), Some(ScanMode::Owned));

        assert!(session.end(&fake).await.unwrap());
        // Second end is a no-op.
        assert!(!session.end(&fake).await.unwrap());

        assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fake.stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_probe_discovery_event_clone() {
        // Just verify the struct is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<ProbeDiscoveryEvent>();
    }
}
