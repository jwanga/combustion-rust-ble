//! Protocol module for parsing and constructing messages.
//!
//! This module contains the implementations for:
//! - UART message parsing and construction
//! - Probe status parsing
//! - CRC calculation

pub mod crc;
pub mod status;
pub mod uart_messages;

pub use crc::calculate_crc;
pub use status::ProbeStatus;
pub use uart_messages::{UartMessage, UartMessageHeader, UartMessageType};
