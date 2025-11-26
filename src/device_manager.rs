//! Device manager for discovering and managing Combustion Predictive Probes.
//!
//! This module handles BLE scanning and probe lifecycle management.
//! Only Predictive Probes (ProductType::PredictiveProbe) are discovered
//! and managed. Other Combustion devices (Display, Booster, MeatNet Repeater,
//! Giant Grill Gauge) are intentionally filtered out.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::ble::scanner::{BleScanner, ProbeDiscoveryEvent};
use crate::error::Result;
use crate::probe::{CallbackHandle, Probe};

/// Maximum number of probes that can be managed simultaneously.
pub const MAX_PROBES: usize = 8;

/// Event emitted when a probe is discovered.
#[derive(Debug, Clone)]
pub struct ProbeEvent {
    /// The probe that was discovered or updated.
    pub identifier: String,
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
    probe_discovered_tx: broadcast::Sender<Arc<Probe>>,
    /// Probe stale channel.
    probe_stale_tx: broadcast::Sender<Arc<Probe>>,
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
    /// # Errors
    ///
    /// Returns an error if Bluetooth is not available.
    pub async fn new() -> Result<Self> {
        let scanner = BleScanner::new().await?;

        let (probe_discovered_tx, _) = broadcast::channel(32);
        let (probe_stale_tx, _) = broadcast::channel(32);

        Ok(Self {
            scanner: Arc::new(scanner),
            probes: Arc::new(RwLock::new(HashMap::new())),
            meatnet_enabled: AtomicBool::new(false),
            probe_discovered_tx,
            probe_stale_tx,
            callback_counter: AtomicU64::new(0),
            background_handle: RwLock::new(None),
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Initialize Bluetooth and start scanning for probes.
    pub async fn start_scanning(&self) -> Result<()> {
        if self.is_running.load(Ordering::SeqCst) {
            debug!("Already scanning");
            return Ok(());
        }

        info!("Starting device manager scanning");

        self.scanner.start_scanning().await?;
        self.is_running.store(true, Ordering::SeqCst);

        // Start background task to process discovery events
        let scanner = self.scanner.clone();
        let probes = self.probes.clone();
        let probe_discovered_tx = self.probe_discovered_tx.clone();
        let probe_stale_tx = self.probe_stale_tx.clone();
        let is_running = self.is_running.clone();

        let handle = tokio::spawn(async move {
            let mut rx = scanner.subscribe();

            while is_running.load(Ordering::SeqCst) {
                tokio::select! {
                    Ok(event) = rx.recv() => {
                        Self::handle_discovery_event(
                            event,
                            &probes,
                            &probe_discovered_tx,
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

        Ok(())
    }

    /// Stop scanning for probes.
    pub async fn stop_scanning(&self) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Stopping device manager scanning");

        self.is_running.store(false, Ordering::SeqCst);
        self.scanner.stop_scanning().await?;

        // Wait for background task
        if let Some(handle) = self.background_handle.write().take() {
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
    pub fn subscribe_probe_discovered(&self) -> broadcast::Receiver<Arc<Probe>> {
        self.probe_discovered_tx.subscribe()
    }

    /// Register a callback for when probes are discovered/updated.
    pub fn on_probe_discovered<F>(&self, callback: F) -> CallbackHandle
    where
        F: Fn(Arc<Probe>) + Send + Sync + 'static,
    {
        let callback_id = self.callback_counter.fetch_add(1, Ordering::SeqCst);
        let mut rx = self.probe_discovered_tx.subscribe();

        let handle = tokio::spawn(async move {
            while let Ok(probe) = rx.recv().await {
                callback(probe);
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
        probe_discovered_tx: &broadcast::Sender<Arc<Probe>>,
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

                // Create new probe
                let probe = Arc::new(Probe::new(
                    ble_identifier.clone(),
                    event.peripheral,
                    serial_number,
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
        let _ = probe_discovered_tx.send(probe);
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
}
