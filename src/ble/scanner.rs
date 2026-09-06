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
pub(crate) trait ScanControl: Send + Sync {
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

/// Substrings that identify BlueZ's `org.bluez.Error.InProgress` reply to
/// `StartDiscovery` once it has passed through `dbus` → `bluez-async` → btleplug.
/// The D-Bus error *name* is not part of the display string, only the message
/// ("Operation already in progress"), so both spellings are checked.
const SCAN_IN_PROGRESS_MARKERS: &[&str] = &["already in progress", "InProgress"];

/// Map a `start_scan` failure to [`Error::ScanInProgress`] when the platform reports
/// that a scan is already running on the adapter; everything else stays
/// [`Error::Bluetooth`].
fn map_start_scan_error(err: btleplug::Error) -> Error {
    if crate::ble::btleplug_error_matches(&err, SCAN_IN_PROGRESS_MARKERS) {
        Error::ScanInProgress
    } else {
        Error::Bluetooth(err)
    }
}

/// Owner-vs-attached scan state machine.
///
/// Kept separate from [`BleScanner`] so the "never call `stop_scan` in attached
/// mode" rule can be tested against a fake [`ScanControl`].
pub(crate) struct ScanSession {
    /// How the current scan was started; `None` while inactive. Shared with the
    /// spawned event loop, which runs while this is `Some`.
    mode: Arc<RwLock<Option<ScanMode>>>,
    /// Serializes `begin` / `end` across their adapter awaits so two concurrent
    /// starts (or a start racing a stop) cannot both pass the state check.
    transition: tokio::sync::Mutex<()>,
}

impl ScanSession {
    fn new() -> Self {
        Self {
            mode: Arc::new(RwLock::new(None)),
            transition: tokio::sync::Mutex::new(()),
        }
    }

    /// The state shared with the event-processing task; the loop runs while `Some`.
    fn shared_mode(&self) -> Arc<RwLock<Option<ScanMode>>> {
        self.mode.clone()
    }

    fn is_active(&self) -> bool {
        self.mode.read().is_some()
    }

    fn mode(&self) -> Option<ScanMode> {
        *self.mode.read()
    }

    /// Force the session inactive without touching the adapter (used on drop).
    fn clear(&self) {
        self.mode.write().take();
    }

    /// Activate the session. In [`ScanMode::Owned`] this starts the adapter scan; in
    /// [`ScanMode::Attached`] the adapter is not touched. Returns `Ok(false)` if the
    /// session was already active in the same mode (no adapter call is made), and
    /// [`Error::ScanModeMismatch`] if it is active in the other mode.
    async fn begin(
        &self,
        control: &dyn ScanControl,
        mode: ScanMode,
        filter: ScanFilter,
    ) -> Result<bool> {
        let _guard = self.transition.lock().await;
        match self.mode() {
            Some(current) if current == mode => return Ok(false),
            Some(current) => {
                return Err(Error::ScanModeMismatch {
                    current,
                    requested: mode,
                })
            }
            None => {}
        }
        if mode == ScanMode::Owned {
            control
                .start_scan(filter)
                .await
                .map_err(map_start_scan_error)?;
        }
        *self.mode.write() = Some(mode);
        Ok(true)
    }

    /// Deactivate the session and return the mode it was in. Calls `stop_scan` only if
    /// this session owned the scan. Returns `Ok(None)` if the session was not active.
    /// If `stop_scan` fails the session stays active in its previous mode so the caller
    /// can retry.
    async fn end(&self, control: &dyn ScanControl) -> Result<Option<ScanMode>> {
        let _guard = self.transition.lock().await;
        let Some(mode) = self.mode.write().take() else {
            return Ok(None);
        };
        if mode == ScanMode::Owned {
            if let Err(e) = control.stop_scan().await {
                *self.mode.write() = Some(mode);
                return Err(Error::Bluetooth(e));
            }
        }
        Ok(Some(mode))
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
    /// Returns [`Error::ScanInProgress`] if the platform reports that a scan is already
    /// running on this adapter (BlueZ `org.bluez.Error.InProgress`); fall back to
    /// [`attach`](Self::attach) in that case. Any other failure is
    /// [`Error::Bluetooth`].
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
    /// Returns [`Error::ScanModeMismatch`] if this scanner already owns a scan it
    /// started via [`start_scanning`](Self::start_scanning).
    pub async fn attach(&self) -> Result<()> {
        self.start_with(ScanMode::Attached, ScanFilter::default())
            .await
    }

    /// Shared start path. Any previous event loop is joined before a new one is
    /// spawned so a failed stop cannot leave two loops feeding the same channels.
    async fn start_with(&self, mode: ScanMode, filter: ScanFilter) -> Result<()> {
        if !self.session.begin(&self.adapter, mode, filter).await? {
            debug!("Already scanning, ignoring start request");
            return Ok(());
        }
        let previous = self.scan_handle.write().take();
        if let Some(handle) = previous {
            let _ = handle.await;
        }

        match mode {
            ScanMode::Owned => info!("Starting BLE scan for Combustion probes"),
            ScanMode::Attached => info!("Attaching to host-owned BLE scan"),
        }

        // Start the event processing task
        let adapter = self.adapter.clone();
        let is_scanning = self.session.shared_mode();
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

            while is_scanning.read().is_some() {
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
                        if is_scanning.read().is_none() {
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
        match self.session.end(&self.adapter).await? {
            None => {
                debug!("Not scanning, ignoring stop request");
                return Ok(());
            }
            Some(ScanMode::Owned) => info!("Stopped BLE scan"),
            Some(ScanMode::Attached) => info!("Detached from host-owned BLE scan"),
        }

        // Wait for the scan task to complete
        let previous = self.scan_handle.write().take();
        if let Some(handle) = previous {
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
        self.session.clear();
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
    async fn test_attached_session_never_calls_start_or_stop_scan() {
        let fake = FakeScan::default();
        let session = ScanSession::new();

        assert!(session
            .begin(&fake, ScanMode::Attached, ScanFilter::default())
            .await
            .unwrap());
        assert_eq!(session.mode(), Some(ScanMode::Attached));
        assert!(session.is_active());

        assert_eq!(session.end(&fake).await.unwrap(), Some(ScanMode::Attached));
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
    async fn test_owned_session_starts_and_stops_adapter_scan_once() {
        let fake = FakeScan::default();
        let session = ScanSession::new();

        assert!(session
            .begin(&fake, ScanMode::Owned, ScanFilter::default())
            .await
            .unwrap());
        // Second begin in the same mode is a no-op and does not touch the adapter.
        assert!(!session
            .begin(&fake, ScanMode::Owned, ScanFilter::default())
            .await
            .unwrap());
        // Switching mode while active is rejected rather than silently ignored.
        assert!(matches!(
            session
                .begin(&fake, ScanMode::Attached, ScanFilter::default())
                .await,
            Err(Error::ScanModeMismatch {
                current: ScanMode::Owned,
                requested: ScanMode::Attached
            })
        ));
        assert_eq!(session.mode(), Some(ScanMode::Owned));

        assert_eq!(session.end(&fake).await.unwrap(), Some(ScanMode::Owned));
        // Second end is a no-op.
        assert_eq!(session.end(&fake).await.unwrap(), None);

        assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fake.stops.load(Ordering::SeqCst), 1);
    }

    /// Fake whose `start_scan` fails the way btleplug's BlueZ backend does when the
    /// host is already discovering: `Error::Other` wrapping a D-Bus error whose display
    /// string is just the message.
    struct HostAlreadyScanning;

    #[async_trait]
    impl ScanControl for HostAlreadyScanning {
        async fn start_scan(&self, _filter: ScanFilter) -> btleplug::Result<()> {
            Err(btleplug::Error::Other(
                "Operation already in progress".into(),
            ))
        }

        async fn stop_scan(&self) -> btleplug::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_start_scan_in_progress_maps_to_scan_in_progress() {
        let session = ScanSession::new();
        let result = session
            .begin(&HostAlreadyScanning, ScanMode::Owned, ScanFilter::default())
            .await;
        assert!(matches!(result, Err(Error::ScanInProgress)));
        // The session must not have been activated.
        assert!(!session.is_active());

        // Attaching afterwards works, which is the intended fallback.
        assert!(session
            .begin(
                &HostAlreadyScanning,
                ScanMode::Attached,
                ScanFilter::default()
            )
            .await
            .unwrap());
        assert_eq!(session.mode(), Some(ScanMode::Attached));
    }

    #[test]
    fn test_map_start_scan_error_only_matches_in_progress() {
        assert!(matches!(
            map_start_scan_error(btleplug::Error::Other("org.bluez.Error.InProgress".into())),
            Error::ScanInProgress
        ));
        assert!(matches!(
            map_start_scan_error(btleplug::Error::Other(
                "Operation already in progress".into()
            )),
            Error::ScanInProgress
        ));
        assert!(matches!(
            map_start_scan_error(btleplug::Error::Other("Resource Not Ready".into())),
            Error::Bluetooth(_)
        ));
        assert!(matches!(
            map_start_scan_error(btleplug::Error::PermissionDenied),
            Error::Bluetooth(_)
        ));
    }

    /// A failing `stop_scan` must leave the session active so the caller can retry,
    /// instead of stranding a running adapter scan behind an inactive session.
    struct FailingStop;

    #[async_trait]
    impl ScanControl for FailingStop {
        async fn start_scan(&self, _filter: ScanFilter) -> btleplug::Result<()> {
            Ok(())
        }

        async fn stop_scan(&self) -> btleplug::Result<()> {
            Err(btleplug::Error::Other("stop failed".into()))
        }
    }

    #[tokio::test]
    async fn test_failed_stop_scan_keeps_owned_session_active() {
        let session = ScanSession::new();
        session
            .begin(&FailingStop, ScanMode::Owned, ScanFilter::default())
            .await
            .unwrap();

        assert!(matches!(
            session.end(&FailingStop).await,
            Err(Error::Bluetooth(_))
        ));
        assert_eq!(session.mode(), Some(ScanMode::Owned));

        // Retrying against a working control stops cleanly.
        let ok = FakeScan::default();
        assert_eq!(session.end(&ok).await.unwrap(), Some(ScanMode::Owned));
        assert_eq!(ok.stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_probe_discovery_event_clone() {
        // Just verify the struct is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<ProbeDiscoveryEvent>();
    }
}
