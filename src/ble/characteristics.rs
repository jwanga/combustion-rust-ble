//! GATT characteristic handling.
//!
//! Provides functionality for reading, writing, and subscribing to
//! BLE characteristics on Combustion probes.

use btleplug::api::{Characteristic, Peripheral as _, WriteType};
use btleplug::platform::Peripheral;
use futures::stream::StreamExt;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, trace};
use uuid::Uuid;

use crate::ble::uuids::*;
use crate::error::{Error, Result};

/// Notification event from a characteristic.
#[derive(Debug, Clone)]
pub struct NotificationEvent {
    /// UUID of the characteristic that sent the notification.
    pub characteristic_uuid: Uuid,
    /// The notification data.
    pub data: Vec<u8>,
}

/// Handler for GATT characteristics on a probe.
pub struct CharacteristicHandler {
    /// The peripheral to communicate with.
    peripheral: Peripheral,
    /// Cached characteristics by UUID.
    characteristics: Arc<RwLock<HashMap<Uuid, Characteristic>>>,
    /// Channel for notification events.
    notification_tx: broadcast::Sender<NotificationEvent>,
    /// Whether we're currently listening for notifications.
    is_listening: Arc<RwLock<bool>>,
    /// Handle to the notification listener task.
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl CharacteristicHandler {
    /// Create a new characteristic handler for a peripheral.
    ///
    /// Note: Services must be discovered before using this handler.
    pub fn new(peripheral: Peripheral) -> Self {
        let (notification_tx, _) = broadcast::channel(256);

        Self {
            peripheral,
            characteristics: Arc::new(RwLock::new(HashMap::new())),
            notification_tx,
            is_listening: Arc::new(RwLock::new(false)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Discover and cache all characteristics.
    ///
    /// This should be called after connecting and discovering services.
    pub async fn discover_characteristics(&self) -> Result<()> {
        let services = self.peripheral.services();

        let mut chars = self.characteristics.write();
        chars.clear();

        for service in services {
            for characteristic in service.characteristics {
                debug!(
                    "Found characteristic: {} in service {}",
                    characteristic.uuid, service.uuid
                );
                chars.insert(characteristic.uuid, characteristic);
            }
        }

        debug!("Discovered {} characteristics", chars.len());

        Ok(())
    }

    /// Get a characteristic by UUID.
    pub fn get_characteristic(&self, uuid: &Uuid) -> Option<Characteristic> {
        self.characteristics.read().get(uuid).cloned()
    }

    /// Check if a characteristic exists.
    pub fn has_characteristic(&self, uuid: &Uuid) -> bool {
        self.characteristics.read().contains_key(uuid)
    }

    /// Read a characteristic value.
    pub async fn read(&self, uuid: &Uuid) -> Result<Vec<u8>> {
        let characteristic = self
            .characteristics
            .read()
            .get(uuid)
            .cloned()
            .ok_or_else(|| Error::CharacteristicNotFound {
                uuid: uuid.to_string(),
            })?;

        let data = self
            .peripheral
            .read(&characteristic)
            .await
            .map_err(Error::Bluetooth)?;

        trace!("Read {} bytes from characteristic {}", data.len(), uuid);

        Ok(data)
    }

    /// Write to a characteristic.
    pub async fn write(&self, uuid: &Uuid, data: &[u8], with_response: bool) -> Result<()> {
        let characteristic = self
            .characteristics
            .read()
            .get(uuid)
            .cloned()
            .ok_or_else(|| Error::CharacteristicNotFound {
                uuid: uuid.to_string(),
            })?;

        let write_type = if with_response {
            WriteType::WithResponse
        } else {
            WriteType::WithoutResponse
        };

        self.peripheral
            .write(&characteristic, data, write_type)
            .await
            .map_err(Error::Bluetooth)?;

        trace!("Wrote {} bytes to characteristic {}", data.len(), uuid);

        Ok(())
    }

    /// Subscribe to notifications from a characteristic.
    pub async fn subscribe(&self, uuid: &Uuid) -> Result<()> {
        debug!("Attempting to subscribe to characteristic: {}", uuid);

        let characteristic = self
            .characteristics
            .read()
            .get(uuid)
            .cloned()
            .ok_or_else(|| {
                debug!(
                    "Characteristic {} NOT found in discovered characteristics",
                    uuid
                );
                // List all discovered characteristics for debugging
                let chars = self.characteristics.read();
                for (k, _) in chars.iter() {
                    debug!("  Available characteristic: {}", k);
                }
                Error::CharacteristicNotFound {
                    uuid: uuid.to_string(),
                }
            })?;

        debug!(
            "Found characteristic {}, properties: {:?}",
            uuid, characteristic.properties
        );

        self.peripheral
            .subscribe(&characteristic)
            .await
            .map_err(|e| {
                debug!("Failed to subscribe to {}: {:?}", uuid, e);
                Error::Bluetooth(e)
            })?;

        debug!("Successfully subscribed to notifications from {}", uuid);

        Ok(())
    }

    /// Unsubscribe from notifications from a characteristic.
    pub async fn unsubscribe(&self, uuid: &Uuid) -> Result<()> {
        let characteristic = self
            .characteristics
            .read()
            .get(uuid)
            .cloned()
            .ok_or_else(|| Error::CharacteristicNotFound {
                uuid: uuid.to_string(),
            })?;

        self.peripheral
            .unsubscribe(&characteristic)
            .await
            .map_err(Error::Bluetooth)?;

        debug!("Unsubscribed from notifications from {}", uuid);

        Ok(())
    }

    /// Start listening for notifications.
    ///
    /// Notifications will be sent through the channel returned by `subscribe_notifications()`.
    pub async fn start_notifications(&self) -> Result<()> {
        if *self.is_listening.read() {
            return Ok(());
        }

        *self.is_listening.write() = true;

        let peripheral = self.peripheral.clone();
        let is_listening = self.is_listening.clone();
        let notification_tx = self.notification_tx.clone();

        let handle = tokio::spawn(async move {
            debug!("Notification listener task starting");

            let mut notifications = match peripheral.notifications().await {
                Ok(n) => {
                    debug!("Got notifications stream successfully");
                    n
                }
                Err(e) => {
                    error!("Failed to get notifications stream: {}", e);
                    return;
                }
            };

            debug!("Notification listener entering main loop");

            while *is_listening.read() {
                tokio::select! {
                    Some(notification) = notifications.next() => {
                        debug!(
                            "Notification received from {}: {} bytes, data: {:02X?}",
                            notification.uuid,
                            notification.value.len(),
                            &notification.value[..std::cmp::min(notification.value.len(), 20)]
                        );

                        let event = NotificationEvent {
                            characteristic_uuid: notification.uuid,
                            data: notification.value,
                        };

                        let send_result = notification_tx.send(event);
                        debug!("Notification broadcast result: {:?}", send_result.is_ok());
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                        // Check if we should stop
                        if !*is_listening.read() {
                            break;
                        }
                    }
                }
            }

            debug!("Notification listener stopped");
        });

        *self.listener_handle.write() = Some(handle);

        Ok(())
    }

    /// Stop listening for notifications.
    pub async fn stop_notifications(&self) {
        *self.is_listening.write() = false;

        if let Some(handle) = self.listener_handle.write().take() {
            let _ = handle.await;
        }
    }

    /// Get a receiver for notification events.
    pub fn subscribe_notifications(&self) -> broadcast::Receiver<NotificationEvent> {
        self.notification_tx.subscribe()
    }

    /// Read a string value from a characteristic.
    pub async fn read_string(&self, uuid: &Uuid) -> Result<String> {
        let data = self.read(uuid).await?;
        String::from_utf8(data).map_err(|_| Error::InvalidData {
            context: format!("Invalid UTF-8 in characteristic {}", uuid),
        })
    }

    /// Read the manufacturer name.
    pub async fn read_manufacturer_name(&self) -> Result<String> {
        self.read_string(&MANUFACTURER_NAME_UUID).await
    }

    /// Read the model number.
    pub async fn read_model_number(&self) -> Result<String> {
        self.read_string(&MODEL_NUMBER_UUID).await
    }

    /// Read the serial number.
    pub async fn read_serial_number(&self) -> Result<String> {
        self.read_string(&SERIAL_NUMBER_UUID).await
    }

    /// Read the firmware revision.
    pub async fn read_firmware_revision(&self) -> Result<String> {
        self.read_string(&FIRMWARE_REVISION_UUID).await
    }

    /// Read the hardware revision.
    pub async fn read_hardware_revision(&self) -> Result<String> {
        self.read_string(&HARDWARE_REVISION_UUID).await
    }

    /// Write to the UART RX characteristic.
    pub async fn write_uart(&self, data: &[u8]) -> Result<()> {
        self.write(&UART_RX_UUID, data, false).await
    }

    /// Subscribe to UART TX notifications.
    pub async fn subscribe_uart(&self) -> Result<()> {
        self.subscribe(&UART_TX_UUID).await
    }
}

impl Drop for CharacteristicHandler {
    fn drop(&mut self) {
        *self.is_listening.write() = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_event_clone() {
        let event = NotificationEvent {
            characteristic_uuid: UART_TX_UUID,
            data: vec![1, 2, 3],
        };
        let cloned = event.clone();
        assert_eq!(event.characteristic_uuid, cloned.characteristic_uuid);
        assert_eq!(event.data, cloned.data);
    }
}
