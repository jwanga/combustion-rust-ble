//! Error types for the combustion-rust-ble crate.

use thiserror::Error;

/// The main error type for this crate.
#[derive(Error, Debug)]
pub enum Error {
    /// Bluetooth-related error from the underlying BLE library.
    #[error("Bluetooth error: {0}")]
    Bluetooth(#[from] btleplug::Error),

    /// Bluetooth is not available or is disabled on this system.
    #[error("Bluetooth not available or disabled")]
    BluetoothUnavailable,

    /// The specified probe was not found.
    #[error("Probe not found: {identifier}")]
    ProbeNotFound {
        /// The identifier that was searched for.
        identifier: String,
    },

    /// Operation requires a connection but the probe is not connected.
    #[error("Probe not connected")]
    NotConnected,

    /// Failed to establish a connection to the probe.
    #[error("Connection failed: {reason}")]
    ConnectionFailed {
        /// Description of why the connection failed.
        reason: String,
    },

    /// The connection to the probe was lost.
    #[error("Connection lost")]
    ConnectionLost,

    /// Invalid data was received from the probe.
    #[error("Invalid data received: {context}")]
    InvalidData {
        /// Description of what was invalid about the data.
        context: String,
    },

    /// CRC check failed for a UART message.
    #[error("CRC mismatch: expected {expected:#06x}, got {actual:#06x}")]
    CrcMismatch {
        /// The expected CRC value.
        expected: u16,
        /// The actual CRC value received.
        actual: u16,
    },

    /// A UART message response timed out.
    #[error("UART message timeout")]
    Timeout,

    /// The requested operation is not supported.
    #[error("Operation not supported: {operation}")]
    NotSupported {
        /// Description of the unsupported operation.
        operation: String,
    },

    /// The probe reported an error.
    #[error("Probe reported error: {message}")]
    ProbeError {
        /// The error message from the probe.
        message: String,
    },

    /// The maximum number of probes has been reached.
    #[error("Maximum probes ({max}) already connected")]
    MaxProbesReached {
        /// The maximum number of probes allowed.
        max: usize,
    },

    /// An invalid parameter was provided.
    #[error("Invalid parameter: {name} = {value}")]
    InvalidParameter {
        /// The name of the parameter.
        name: String,
        /// The invalid value that was provided.
        value: String,
    },

    /// An internal error occurred.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Characteristic not found on the device.
    #[error("Characteristic not found: {uuid}")]
    CharacteristicNotFound {
        /// The UUID of the characteristic that was not found.
        uuid: String,
    },

    /// Service not found on the device.
    #[error("Service not found: {uuid}")]
    ServiceNotFound {
        /// The UUID of the service that was not found.
        uuid: String,
    },
}

/// A specialized Result type for this crate.
pub type Result<T> = std::result::Result<T, Error>;
