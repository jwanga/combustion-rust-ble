//! BLE connection management.
//!
//! Handles connecting to and maintaining connections with Combustion probes.

use btleplug::api::Peripheral as _;
use btleplug::platform::Peripheral;
use parking_lot::RwLock;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Semaphore};
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

/// Default timeout to wait for BlueZ `ServicesResolved` after a successful link-layer
/// connect. Weak-RSSI peripherals can take a noticeable fraction of a second to enumerate
/// their GATT tree; subscribing too early yields stale D-Bus paths.
const DEFAULT_SERVICES_RESOLVED_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// How long to wait for services to resolve after link-up.
    services_resolved_timeout: Duration,
    /// Adapter-level permit that serializes `peripheral.connect()` calls. BlueZ rejects a
    /// second `org.bluez.Device1.Connect()` while a previous one is in flight with
    /// `DBus_Error_InProgress` ("Operation already in progress"), so all connect calls on
    /// the same adapter must funnel through this single-permit semaphore. Cloned from
    /// [`DeviceManager`] and shared across every [`ConnectionManager`] managed by the
    /// same adapter.
    connect_permit: Arc<Semaphore>,
}

impl ConnectionManager {
    /// Create a new connection manager for a peripheral.
    ///
    /// `connect_permit` is the adapter-level single-permit semaphore that serializes
    /// BlueZ `Connect()` calls. All `ConnectionManager`s managed by the same adapter
    /// must be constructed with `Arc::clone`s of the same semaphore.
    pub fn new(peripheral: Peripheral, connect_permit: Arc<Semaphore>) -> Self {
        let (event_tx, _) = broadcast::channel(16);

        Self {
            peripheral,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            maintain_connection: Arc::new(RwLock::new(false)),
            event_tx,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(1),
            services_resolved_timeout: DEFAULT_SERVICES_RESOLVED_TIMEOUT,
            connect_permit,
        }
    }

    /// Number of permits currently available on the adapter-level connect semaphore.
    /// Exposed for telemetry and tests — never construct a permit from outside this
    /// module; the semaphore itself stays internal.
    pub fn connect_permits_available(&self) -> usize {
        self.connect_permit.available_permits()
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
            // Defense-in-depth: internal state can lag behind a real link-layer drop if
            // a `CentralEvent::DeviceDisconnected` was missed or arrived after we took
            // our snapshot. Double-check BlueZ before short-circuiting.
            if self.peripheral.is_connected().await.unwrap_or(false) {
                debug!("Already connected");
                return Ok(());
            }
            warn!(
                peripheral_id = ?self.peripheral.id(),
                "Stale internal Connected state — peripheral disconnected at link layer. Resetting and reconnecting.",
            );
            self.set_state(ConnectionState::Disconnected);
            // Fall through to the full connect+resolve path below.
        } else if current_state.is_transitioning() {
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

        let link_start = std::time::Instant::now();
        let mut last_attempt_error: Option<Error> = None;

        while attempts < max_attempts {
            attempts += 1;

            debug!("Connection attempt {} of {}", attempts, max_attempts);

            // Serialize BlueZ Connect() calls across this adapter — see field comment on
            // `connect_permit`. The permit is held ONLY for the Connect D-Bus round-trip;
            // it is released before `resolve_services` and all subsequent GATT discovery
            // / subscribe work, which BlueZ allows in parallel across devices.
            let connect_result =
                connect_with_permit(&self.connect_permit, self.peripheral.connect()).await;

            match connect_result {
                Ok(_) => {
                    info!(
                        step = "link_established",
                        attempts = attempts,
                        duration_ms = link_start.elapsed().as_millis() as u64,
                        "Link-layer connection established",
                    );

                    // Discover services and wait for the GATT tree to be fully resolved.
                    // Service resolution can fail transiently on weak-RSSI peripherals
                    // (ServicesResolved never fires within the timeout). Treat that like
                    // a link-layer failure — disconnect cleanly and consume one retry —
                    // rather than aborting the loop after a single attempt.
                    match self.resolve_services().await {
                        Ok(()) => {
                            self.set_state(ConnectionState::Connected);
                            return Ok(());
                        }
                        Err(e) => {
                            warn!(
                                attempts = attempts,
                                error = %e,
                                "Service resolution failed; disconnecting and retrying",
                            );
                            let _ = self.peripheral.disconnect().await;
                            last_attempt_error = Some(e);
                            if attempts < max_attempts {
                                tokio::time::sleep(self.reconnect_delay).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempts, e);
                    last_attempt_error = Some(Error::Bluetooth(e));

                    if attempts < max_attempts {
                        tokio::time::sleep(self.reconnect_delay).await;
                    }
                }
            }
        }

        self.set_state(ConnectionState::Disconnected);
        Err(
            last_attempt_error.unwrap_or_else(|| Error::ConnectionFailed {
                reason: format!("Failed after {} attempts", max_attempts),
            }),
        )
    }

    /// Discover services and wait for the underlying BLE stack to fully resolve the GATT
    /// tree. Returns Err if discovery fails, the peripheral disconnects, or the timeout
    /// expires before any services appear.
    async fn resolve_services(&self) -> Result<()> {
        let discover_start = std::time::Instant::now();

        self.peripheral
            .discover_services()
            .await
            .map_err(|e| Error::ConnectionFailed {
                reason: format!("Service discovery failed: {e}"),
            })?;

        // Poll until services() reports a non-empty list (BlueZ ServicesResolved proxy) or
        // the peripheral disconnects or we time out.
        let poll_interval = Duration::from_millis(50);
        let deadline = std::time::Instant::now() + self.services_resolved_timeout;

        loop {
            let connected = self.peripheral.is_connected().await.unwrap_or(false);
            if !connected {
                return Err(Error::ConnectionFailed {
                    reason: "peripheral disconnected before services resolved".into(),
                });
            }

            let service_count = self.peripheral.services().len();
            if service_count > 0 {
                info!(
                    step = "services_discovered",
                    count = service_count,
                    duration_ms = discover_start.elapsed().as_millis() as u64,
                    "Services resolved",
                );
                return Ok(());
            }

            if std::time::Instant::now() >= deadline {
                return Err(Error::ConnectionFailed {
                    reason: format!(
                        "services not resolved within {}ms",
                        self.services_resolved_timeout.as_millis()
                    ),
                });
            }

            tokio::time::sleep(poll_interval).await;
        }
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

    /// Set how long the connect path waits for `ServicesResolved` after a successful
    /// link-layer connect. Defaults to 5s. Increase on congested 2.4 GHz environments or
    /// for very weak-RSSI peripherals; decrease to fail fast.
    pub fn set_services_resolved_timeout(&mut self, timeout: Duration) {
        self.services_resolved_timeout = timeout;
    }

    /// Reset internal state to `Disconnected` and emit the corresponding event.
    ///
    /// Use this when an out-of-band signal (e.g. `CentralEvent::DeviceDisconnected` routed
    /// from the scanner) tells the library that the link is gone. This is sync and
    /// non-blocking; it never auto-reconnects. Reconnect policy belongs to the caller.
    pub fn reset_to_disconnected(&self) {
        self.set_state(ConnectionState::Disconnected);
    }

    /// Handle a disconnection event (called externally when disconnect is detected).
    ///
    /// Always resets state to `Disconnected` first. If `maintain_connection` was set on
    /// the last `connect()` call, also attempts to reconnect. This method is preserved
    /// for back-compat; new callers should prefer [`Self::reset_to_disconnected`] and
    /// drive their own reconnect policy.
    pub async fn handle_disconnection(&self) {
        self.reset_to_disconnected();

        if !*self.maintain_connection.read() {
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
        transition_state(
            &self.state,
            new_state,
            &self.event_tx,
            &format!("{:?}", self.peripheral.id()),
        );
    }
}

/// Acquire the adapter-level connect permit, await the given future, then release.
///
/// Extracted as a free function for two reasons:
///
/// 1. Production callers ([`ConnectionManager::connect`]) hand it the unstarted
///    `peripheral.connect()` future. Since async fns are lazy, the BlueZ
///    `org.bluez.Device1.Connect()` D-Bus call only fires when the future is polled,
///    which only happens inside this scope — i.e. while the permit is held.
/// 2. The serialization guarantee can be unit-tested by handing it a fake future that
///    increments a counter on entry and decrements on exit, without needing to
///    construct a real `btleplug::Peripheral`.
///
/// The permit is released by RAII when `_permit` goes out of scope, so cancellation
/// safety is automatic: if the outer future is dropped mid-acquire or mid-`fut.await`,
/// the permit is released.
async fn connect_with_permit<F, T>(permit: &Semaphore, fut: F) -> T
where
    F: Future<Output = T>,
{
    let _permit = permit
        .acquire()
        .await
        .expect("connect_permit semaphore was closed unexpectedly");
    fut.await
}

/// Set `state` to `new_state` and broadcast a [`ConnectionEvent`] if the value changed.
///
/// Extracted as a free function so the state-transition behavior can be unit-tested
/// without constructing a real [`Peripheral`] (which btleplug doesn't expose a
/// constructor for outside of adapter scanning).
fn transition_state(
    state: &Arc<RwLock<ConnectionState>>,
    new_state: ConnectionState,
    event_tx: &broadcast::Sender<ConnectionEvent>,
    peripheral_id_debug: &str,
) {
    let old_state = {
        let mut s = state.write();
        let old = *s;
        *s = new_state;
        old
    };

    if old_state != new_state {
        debug!("Connection state changed: {} -> {}", old_state, new_state);

        let _ = event_tx.send(ConnectionEvent {
            identifier: peripheral_id_debug.to_string(),
            state: new_state,
        });
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

    /// LIB-7: `transition_state` (the testable extraction of `reset_to_disconnected`'s
    /// underlying behavior) must move state to `Disconnected` from any prior state and
    /// broadcast a `ConnectionEvent` reflecting the new state — independent of any
    /// `maintain_connection` setting (which lives on `ConnectionManager`, not here).
    #[tokio::test]
    async fn test_transition_state_resets_to_disconnected() {
        for initial in [
            ConnectionState::Connected,
            ConnectionState::Connecting,
            ConnectionState::Disconnecting,
        ] {
            let state = Arc::new(RwLock::new(initial));
            let (tx, mut rx) = broadcast::channel::<ConnectionEvent>(8);

            transition_state(
                &state,
                ConnectionState::Disconnected,
                &tx,
                "test-peripheral",
            );

            assert_eq!(
                *state.read(),
                ConnectionState::Disconnected,
                "state should be Disconnected (initial={:?})",
                initial,
            );

            let evt = rx
                .recv()
                .await
                .expect("a ConnectionEvent must be broadcast on transition");
            assert_eq!(evt.state, ConnectionState::Disconnected);
            assert_eq!(evt.identifier, "test-peripheral");
        }
    }

    /// LIB-7: identical inputs (no-op transition) must not broadcast a duplicate event.
    /// Otherwise subscribers would see spurious "Disconnected -> Disconnected" events
    /// after the disconnect router fires twice in a row (e.g. defensive-check path AND
    /// scanner-routed path on the same drop).
    #[tokio::test]
    async fn test_transition_state_no_event_on_identical_transition() {
        let state = Arc::new(RwLock::new(ConnectionState::Disconnected));
        let (tx, mut rx) = broadcast::channel::<ConnectionEvent>(8);

        transition_state(&state, ConnectionState::Disconnected, &tx, "p");

        // No event should have been queued.
        assert!(
            rx.try_recv().is_err(),
            "no event must be broadcast when the state did not change",
        );
        assert_eq!(*state.read(), ConnectionState::Disconnected);
    }

    /// LIB-8: four concurrent connect attempts sharing one adapter-level semaphore must
    /// be serialized — at most one connect future executes inside the permit-held region
    /// at any moment.
    ///
    /// We can't construct a real `btleplug::Peripheral` in tests, so we exercise
    /// production's `connect_with_permit` directly with a fake future. That function is
    /// the same code path `ConnectionManager::connect` uses to wrap
    /// `self.peripheral.connect()`, so passing this test means the production
    /// serialization holds.
    ///
    /// Sanity check: remove `permit.acquire().await` inside `connect_with_permit` and
    /// this test fails (max_concurrent observed > 1).
    #[tokio::test]
    async fn test_connect_permit_serializes_four_concurrent_connects() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let permit = Arc::new(Semaphore::new(1));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let completions = Arc::new(AtomicUsize::new(0));

        let make_task = |permit: Arc<Semaphore>,
                         active: Arc<AtomicUsize>,
                         max_active: Arc<AtomicUsize>,
                         completions: Arc<AtomicUsize>| async move {
            let active_in_fut = active.clone();
            let max_active_in_fut = max_active.clone();
            // Fake `peripheral.connect()` — increments the in-flight counter on entry,
            // sleeps long enough that contending tasks have time to race, then
            // decrements. Production passes `self.peripheral.connect()` here instead.
            let fake_connect = async move {
                let now_active = active_in_fut.fetch_add(1, Ordering::SeqCst) + 1;
                max_active_in_fut.fetch_max(now_active, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                active_in_fut.fetch_sub(1, Ordering::SeqCst);
            };
            connect_with_permit(&permit, fake_connect).await;
            completions.fetch_add(1, Ordering::SeqCst);
        };

        let (_, _, _, _) = tokio::join!(
            make_task(
                permit.clone(),
                active.clone(),
                max_active.clone(),
                completions.clone()
            ),
            make_task(
                permit.clone(),
                active.clone(),
                max_active.clone(),
                completions.clone()
            ),
            make_task(
                permit.clone(),
                active.clone(),
                max_active.clone(),
                completions.clone()
            ),
            make_task(
                permit.clone(),
                active.clone(),
                max_active.clone(),
                completions.clone()
            ),
        );

        assert_eq!(
            max_active.load(Ordering::SeqCst),
            1,
            "at most one connect future may run concurrently under a 1-permit semaphore",
        );
        assert_eq!(
            completions.load(Ordering::SeqCst),
            4,
            "all four tasks must complete (none deadlocked on the semaphore)",
        );
        assert_eq!(
            permit.available_permits(),
            1,
            "permit must be released after each connect (RAII drop)",
        );
    }

    /// LIB-8: if the future passed to `connect_with_permit` is dropped mid-execution
    /// (cancellation), the permit must still be released. tokio's `SemaphorePermit`
    /// implements `Drop` for exactly this, but the test pins down that property.
    ///
    /// Synchronization uses a `Notify` rather than a fixed sleep so the test is
    /// deterministic on slow / loaded CI — the inner future signals the moment it has
    /// entered the permit-held region.
    #[tokio::test]
    async fn test_connect_permit_released_on_cancellation() {
        use tokio::sync::Notify;

        let permit = Arc::new(Semaphore::new(1));
        let entered = Arc::new(Notify::new());

        let permit_for_task = permit.clone();
        let entered_for_task = entered.clone();
        let handle = tokio::spawn(async move {
            connect_with_permit(&permit_for_task, async move {
                entered_for_task.notify_one();
                // Sleep long enough that we definitely have to abort to interrupt.
                tokio::time::sleep(Duration::from_secs(30)).await;
            })
            .await;
        });

        // Deterministically wait until the task is inside the permit-held region.
        entered.notified().await;
        assert_eq!(
            permit.available_permits(),
            0,
            "permit must be held by the running task",
        );

        // Cancel mid-acquire.
        handle.abort();
        let _ = handle.await; // Allow the task to fully unwind.

        // RAII drop on cancellation releases the permit.
        assert_eq!(
            permit.available_permits(),
            1,
            "permit must be released when the holder is cancelled",
        );
    }
}
