//! BLE scanning functionality.
//!
//! Provides the scanner for discovering Combustion probes.

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral};
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

/// BLE scanner for discovering Combustion probes.
pub struct BleScanner {
    /// The BLE adapter to use for scanning.
    adapter: Adapter,
    /// Whether scanning is currently active.
    is_scanning: Arc<RwLock<bool>>,
    /// Discovered peripherals.
    discovered: Arc<RwLock<HashMap<String, ProbeDiscoveryEvent>>>,
    /// Channel for discovery events.
    event_tx: broadcast::Sender<ProbeDiscoveryEvent>,
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

        let (event_tx, _) = broadcast::channel(100);

        Ok(Self {
            adapter,
            is_scanning: Arc::new(RwLock::new(false)),
            discovered: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            scan_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Create a new BLE scanner with a specific adapter.
    pub fn with_adapter(adapter: Adapter) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            adapter,
            is_scanning: Arc::new(RwLock::new(false)),
            discovered: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            scan_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Start scanning for probes.
    ///
    /// # Errors
    ///
    /// Returns an error if scanning cannot be started.
    pub async fn start_scanning(&self) -> Result<()> {
        if *self.is_scanning.read() {
            debug!("Already scanning, ignoring start request");
            return Ok(());
        }

        info!("Starting BLE scan for Combustion probes");

        // Start the BLE scan
        self.adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(Error::Bluetooth)?;

        *self.is_scanning.write() = true;

        // Start the event processing task
        let adapter = self.adapter.clone();
        let is_scanning = self.is_scanning.clone();
        let discovered = self.discovered.clone();
        let event_tx = self.event_tx.clone();

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

    /// Stop scanning for probes.
    pub async fn stop_scanning(&self) -> Result<()> {
        if !*self.is_scanning.read() {
            debug!("Not scanning, ignoring stop request");
            return Ok(());
        }

        info!("Stopping BLE scan");

        *self.is_scanning.write() = false;

        self.adapter.stop_scan().await.map_err(Error::Bluetooth)?;

        // Wait for the scan task to complete
        if let Some(handle) = self.scan_handle.write().take() {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Check if currently scanning.
    pub fn is_scanning(&self) -> bool {
        *self.is_scanning.read()
    }

    /// Get all discovered probes.
    pub fn discovered_probes(&self) -> HashMap<String, ProbeDiscoveryEvent> {
        self.discovered.read().clone()
    }

    /// Subscribe to discovery events.
    pub fn subscribe(&self) -> broadcast::Receiver<ProbeDiscoveryEvent> {
        self.event_tx.subscribe()
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
        *self.is_scanning.write() = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_discovery_event_clone() {
        // Just verify the struct is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<ProbeDiscoveryEvent>();
    }
}
