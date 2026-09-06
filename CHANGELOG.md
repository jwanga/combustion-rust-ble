# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `DeviceManager::attach()` / `BleScanner::attach()` — process discovery events on a
  scan the host application already runs, without calling `Adapter::start_scan`.
  `stop_scanning` / `shutdown` after `attach` stop event processing but never call
  `Adapter::stop_scan`.
- `ScanMode` (`Owned` | `Attached`) and `DeviceManager::scan_mode()` /
  `BleScanner::scan_mode()` to report how the current scan was started.
- `Error::ScanModeMismatch { current, requested }`, returned when `attach` is called
  on a manager that owns its scan or vice versa. **Breaking:** `Error` is not
  `#[non_exhaustive]`, so exhaustive `match` arms on `Error` must add a handler.

### Fixed

- A failing `Adapter::stop_scan` no longer strands the scanner in an inactive state
  with the adapter scan still running; the session stays active so `stop_scanning`
  can be retried, and a later start never leaves two event loops running.

## [0.1.0] - 2026-09-05

### Added

- `release.sh` and `.github/workflows/release.yml`: pushing a `v<semver>` tag now
  publishes the crate to crates.io automatically. `.github/release.yml` categorizes
  generated GitHub release notes by PR label.
- `DeviceManager::with_adapter(Adapter)` — construct the manager on a btleplug
  `Adapter` the application already owns, instead of opening a second `Manager`.
  `DeviceManager::adapter()` returns the adapter in use.
- `combustion_rust_ble::btleplug` re-export so callers can name the exact btleplug
  version this crate links against.
- `existing_adapter` example demonstrating the above.
- `Error::DeviceDisconnectedDuringSetup { context }` variant, returned when the
  peripheral disconnects between service discovery and characteristic subscription. Callers
  should treat this as "fully disconnect + reconnect" rather than retrying with the same
  cached `Probe` instance. **Breaking:** `Error` is not `#[non_exhaustive]`, so downstream
  code with exhaustive `match` arms on `Error` must add a handler for this variant.
- `DiscoveredProbeEvent { probe, rssi }` carries the RSSI of the triggering advertisement
  packet so consumers can correlate connection failures to signal strength without reading
  it back from cached state.
- `ConnectionManager::set_services_resolved_timeout(Duration)` to tune how long the connect
  path waits for BlueZ `ServicesResolved` after a successful link-layer connect (default
  5s).
- `ConnectionManager::reset_to_disconnected()` — sync method that flips internal state to
  `Disconnected` and emits the corresponding `ConnectionEvent`, without auto-reconnect.
  Use when an out-of-band signal (e.g. `CentralEvent::DeviceDisconnected` routed from the
  scanner) tells the library the link is gone.
- `Probe::handle_link_disconnect()` — forwards to the above and clears the cached
  `CharacteristicHandler`, leaving the probe in a clean state for the next `connect()`.
- `DeviceManager::subscribe_probe_disconnected()` and
  `DeviceManager::on_probe_disconnected(F)` — receive `DisconnectedProbeEvent { probe }`
  notifications when the platform reports a known probe went offline. The event type is
  a struct (rather than `Arc<Probe>` directly) for symmetry with `DiscoveredProbeEvent`
  and to allow future additions without another breaking change.
- `DisconnectedProbeEvent { probe }` payload for the disconnect channel.
- `BleScanner::subscribe_disconnects()` — lower-level channel exposing `PeripheralId`
  values from `CentralEvent::DeviceDisconnected`.
- Adapter-level `connect_permit: Arc<tokio::sync::Semaphore>` (permits=1) inside
  `DeviceManager`, shared with every `Probe`'s `ConnectionManager`. Serializes BlueZ
  `org.bluez.Device1.Connect()` calls so concurrent multi-probe connect attempts no
  longer race and trigger `DBus_Error_InProgress` ("Operation already in progress").
- `ConnectionManager::connect_permits_available()` getter for telemetry / tests.
- Probe discovery via BLE advertising packets
- Real-time temperature reading from all 8 sensors
- Virtual temperature calculation (Core, Surface, Ambient)
- Temperature log download and storage
- Prediction engine integration
- Food safety (SafeCook/USDA Safe) feature support
- Battery status monitoring
- Overheat detection and alerts
- Probe identification (ID 1-8, color assignment)
- Session information management
- Support for up to 8 simultaneous probes
- Cross-platform support (macOS, Windows, Linux, iOS, Android)
- Comprehensive documentation
- Example applications

### Changed

- **Breaking:** `DeviceManager::subscribe_probe_discovered()` now returns
  `broadcast::Receiver<DiscoveredProbeEvent>` instead of `broadcast::Receiver<Arc<Probe>>`.
- **Breaking:** `DeviceManager::on_probe_discovered(F)` callback signature is now
  `F: Fn(DiscoveredProbeEvent) + Send + Sync + 'static`. Access the probe via
  `event.probe` and the RSSI via `event.rssi`.
- **Breaking:** `ConnectionManager::new` now takes
  `(peripheral, connect_permit: Arc<Semaphore>)`. External callers don't normally
  construct this directly — `DeviceManager` and `Probe::new` do — but the path-dep
  integration in `fuego` is unaffected since probes flow exclusively through the
  manager.
- **Breaking (crate-internal):** `Probe::new` (already `pub(crate)`) now takes
  `(identifier, peripheral, serial_number, connect_permit)`.
- Connect path now logs each step (`link_established`, `services_discovered`,
  `characteristics_cached`, `subscribed_uart`, `subscribed_status`,
  `notifications_started`, `connected`) at `info!` with structured fields, so
  `RUST_LOG=combustion_rust_ble=info` is sufficient to diagnose where a connect attempt
  failed.
- Bumped `btleplug` from 0.11 to 0.13 (the crate was developed against 0.11 but never published with it). The scanner now handles the
  `CentralEvent::RssiUpdate` (refreshes RSSI on known probes) and
  `CentralEvent::DeviceServicesModified` variants introduced in 0.12.
- Minimum supported Rust version raised from 1.70 to 1.85, matching the
  dependency tree's requirements.

### Fixed

- `ConnectionManager::connect()` no longer silently swallows `discover_services()` errors.
  A failure now disconnects the half-open link and returns
  `Error::ConnectionFailed { reason: ... }` with the underlying cause.
- After link-up the connect path now polls until BlueZ has actually populated the GATT
  tree (services count > 0) or `services_resolved_timeout` expires. Previously, subscribing
  immediately after `discover_services()` could race the BlueZ ServicesResolved signal on
  weak-RSSI peripherals and yield stale D-Bus paths — observed as
  `Method "StartNotify" ... doesn't exist (org.freedesktop.DBus.Error.UnknownObject)`.
- `CharacteristicHandler::discover_characteristics()` now re-runs service discovery
  itself (rather than trusting btleplug's cache) and returns
  `Error::ConnectionFailed` if the GATT tree is empty or the required UART TX and Probe
  Status characteristics are missing.
- `CharacteristicHandler::subscribe()` detects stale-GATT errors (D-Bus
  `UnknownObject` / `No such interface` / `doesn't exist`) and returns
  `Error::DeviceDisconnectedDuringSetup`, clearing the cached characteristic map so a
  reconnect rebuilds it from scratch instead of reusing dead handles.
- `ConnectionManager::connect()` now double-checks `peripheral.is_connected()` before
  trusting an internal `Connected` snapshot. Previously, a missed
  `CentralEvent::DeviceDisconnected` would leave the internal state stuck at `Connected`
  and the early-out at the top of `connect()` would return `Ok(())` against a dead link,
  causing every downstream operation to fail against an unresolved GATT tree
  (`count=0, has_uart=false, has_probe_status=false`).
- `BleScanner` now routes `CentralEvent::DeviceDisconnected` through a broadcast channel
  rather than just logging it at `debug!`. `DeviceManager` subscribes to this channel,
  finds the matching `Probe` by BLE identifier, and calls
  `Probe::handle_link_disconnect()` to keep internal state honest. The previously dead
  code path (`ConnectionManager::handle_disconnection`) is preserved for back-compat but
  no longer the only line of defense.
- Concurrent multi-probe connect attempts no longer race against BlueZ's HCI command
  queue. Previously, calling `Probe::connect()` on N probes simultaneously caused N-1
  of them to fail with `DBus_Error_InProgress` ("Operation already in progress");
  each retry burned the per-connect retry budget, so one or two of four probes typically
  ended up "the unlucky one" that lost every race and never connected in the initial
  burst. The new adapter-level semaphore funnels Connect() calls one at a time;
  unrelated GATT operations (service discovery, characteristic subscription, etc.) are
  NOT serialized and continue running concurrently across probes.

[Unreleased]: https://github.com/jwanga/combustion-rust-ble/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jwanga/combustion-rust-ble/releases/tag/v0.1.0
