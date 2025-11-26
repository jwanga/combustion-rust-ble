# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2024-XX-XX

### Added

- Initial release
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

### Dependencies

- `btleplug` 0.11 for cross-platform BLE
- `tokio` for async runtime
- `thiserror` for error handling
- `tracing` for logging
- Optional `serde` for serialization

[Unreleased]: https://github.com/combustion-inc/combustion-rust-ble/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/combustion-inc/combustion-rust-ble/releases/tag/v0.1.0
