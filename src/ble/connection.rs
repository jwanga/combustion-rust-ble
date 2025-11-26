//! BLE connection management.
//!
//! Handles connecting to and maintaining connections with Combustion probes.

use btleplug::api::Peripheral as _;
use btleplug::platform::Peripheral;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::error::{Error, Result};

/// Connection state for a probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ConnectionState {
    /// Not connected to the probe.
    #[default]
    Disconnected,
    /// Currently attempting to connect.
    Connecting,
    /// Connected to the probe.
    Connected,
    /// Currently disconnecting.
    Disconnecting,
}

impl ConnectionState {
    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Check if in a transitional state.
    pub fn is_transitioning(&self) -> bool {
        matches!(self, Self::Connecting | Self::Disconnecting)
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected => write!(f, "Connected"),
            Self::Disconnecting => write!(f, "Disconnecting"),
        }
    }
}

/// Event for connection state changes.
#[derive(Debug, Clone)]
pub struct ConnectionEvent {
    /// The identifier of the peripheral.
    pub identifier: String,
    /// The new connection state.
    pub state: ConnectionState,
}

/// Manages connections to Combustion probes.
pub struct ConnectionManager {
    /// The peripheral to manage.
    peripheral: Peripheral,
    /// Current connection state.
    state: Arc<RwLock<ConnectionState>>,
    /// Whether to maintain the connection (auto-reconnect).
    maintain_connection: Arc<RwLock<bool>>,
    /// Channel for connection events.
    event_tx: broadcast::Sender<ConnectionEvent>,
    /// Maximum reconnection attempts.
    max_reconnect_attempts: u32,
    /// Reconnection delay.
    reconnect_delay: Duration,
}

impl ConnectionManager {
    /// Create a new connection manager for a peripheral.
    pub fn new(peripheral: Peripheral) -> Self {
        let (event_tx, _) = broadcast::channel(16);

        Self {
            peripheral,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            maintain_connection: Arc::new(RwLock::new(false)),
            event_tx,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(1),
        }
    }

    /// Get the current connection state.
    pub fn state(&self) -> ConnectionState {
        *self.state.read()
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.state().is_connected()
    }

    /// Subscribe to connection events.
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEvent> {
        self.event_tx.subscribe()
    }

    /// Get the peripheral.
    pub fn peripheral(&self) -> &Peripheral {
        &self.peripheral
    }

    /// Attempt to connect to the probe.
    ///
    /// # Arguments
    ///
    /// * `maintain` - Whether to maintain the connection (auto-reconnect on disconnect)
    pub async fn connect(&self, maintain: bool) -> Result<()> {
        let current_state = *self.state.read();

        if current_state.is_connected() {
            debug!("Already connected");
            return Ok(());
        }

        if current_state.is_transitioning() {
            return Err(Error::ConnectionFailed {
                reason: "Connection already in progress".to_string(),
            });
        }

        *self.maintain_connection.write() = maintain;

        self.set_state(ConnectionState::Connecting);

        // Check if already connected at BLE level
        if self.peripheral.is_connected().await.unwrap_or(false) {
            info!("Peripheral already connected at BLE level");
            self.set_state(ConnectionState::Connected);
            return Ok(());
        }

        // Attempt connection with retries
        let mut attempts = 0;
        let max_attempts = if maintain {
            self.max_reconnect_attempts
        } else {
            1
        };

        while attempts < max_attempts {
            attempts += 1;

            debug!("Connection attempt {} of {}", attempts, max_attempts);

            match self.peripheral.connect().await {
                Ok(_) => {
                    info!("Successfully connected to probe");

                    // Discover services
                    if let Err(e) = self.peripheral.discover_services().await {
                        warn!("Failed to discover services: {}", e);
                    }

                    self.set_state(ConnectionState::Connected);
                    return Ok(());
                }
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempts, e);

                    if attempts < max_attempts {
                        tokio::time::sleep(self.reconnect_delay).await;
                    }
                }
            }
        }

        self.set_state(ConnectionState::Disconnected);
        Err(Error::ConnectionFailed {
            reason: format!("Failed after {} attempts", max_attempts),
        })
    }

    /// Disconnect from the probe.
    pub async fn disconnect(&self) -> Result<()> {
        *self.maintain_connection.write() = false;

        let current_state = *self.state.read();

        if matches!(current_state, ConnectionState::Disconnected) {
            return Ok(());
        }

        if current_state == ConnectionState::Disconnecting {
            return Ok(());
        }

        self.set_state(ConnectionState::Disconnecting);

        match self.peripheral.disconnect().await {
            Ok(_) => {
                info!("Successfully disconnected from probe");
                self.set_state(ConnectionState::Disconnected);
                Ok(())
            }
            Err(e) => {
                error!("Failed to disconnect: {}", e);
                self.set_state(ConnectionState::Disconnected);
                Err(Error::Bluetooth(e))
            }
        }
    }

    /// Check if we're maintaining the connection.
    pub fn is_maintaining_connection(&self) -> bool {
        *self.maintain_connection.read()
    }

    /// Set the reconnection parameters.
    pub fn set_reconnect_params(&mut self, max_attempts: u32, delay: Duration) {
        self.max_reconnect_attempts = max_attempts;
        self.reconnect_delay = delay;
    }

    /// Handle a disconnection event (called externally when disconnect is detected).
    pub async fn handle_disconnection(&self) {
        if !*self.maintain_connection.read() {
            self.set_state(ConnectionState::Disconnected);
            return;
        }

        info!("Connection lost, attempting to reconnect...");

        // Attempt to reconnect
        if let Err(e) = self.connect(true).await {
            error!("Reconnection failed: {}", e);
        }
    }

    /// Update the connection state and emit an event.
    fn set_state(&self, new_state: ConnectionState) {
        let old_state = {
            let mut state = self.state.write();
            let old = *state;
            *state = new_state;
            old
        };

        if old_state != new_state {
            debug!("Connection state changed: {} -> {}", old_state, new_state);

            let _ = self.event_tx.send(ConnectionEvent {
                identifier: format!("{:?}", self.peripheral.id()),
                state: new_state,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state() {
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());

        assert!(ConnectionState::Connecting.is_transitioning());
        assert!(ConnectionState::Disconnecting.is_transitioning());
        assert!(!ConnectionState::Connected.is_transitioning());
    }

    #[test]
    fn test_connection_state_display() {
        assert_eq!(format!("{}", ConnectionState::Connected), "Connected");
        assert_eq!(format!("{}", ConnectionState::Disconnected), "Disconnected");
    }
}
