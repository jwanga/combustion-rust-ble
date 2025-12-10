//! Thermometer preferences and power mode data structures.
//!
//! Contains types for managing power mode and other thermometer preferences.
//! Based on the Combustion Probe BLE Specification.

/// Power mode for the thermometer.
///
/// 2-bit enumeration controlling auto power-off behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum PowerMode {
    /// Normal mode - probe will auto power-off when placed in charger.
    #[default]
    Normal = 0,
    /// Always on mode - probe stays powered even in charger.
    AlwaysOn = 1,
}

impl PowerMode {
    /// Create from raw value.
    pub fn from_raw(value: u8) -> Self {
        match value & 0x03 {
            0 => Self::Normal,
            1 => Self::AlwaysOn,
            _ => Self::Normal, // Reserved values default to Normal
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }

    /// Check if this is always-on mode.
    pub fn is_always_on(&self) -> bool {
        matches!(self, Self::AlwaysOn)
    }

    /// Get a human-readable name for this mode.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::AlwaysOn => "Always On",
        }
    }
}

/// Thermometer preferences.
///
/// This is a 1-byte packed structure containing thermometer settings.
/// Currently only power mode is defined, with remaining bits reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ThermometerPreferences {
    /// Current power mode setting.
    pub power_mode: PowerMode,
    /// Reserved bits (for future use).
    reserved: u8,
}

impl ThermometerPreferences {
    /// Size of the preferences structure in bytes.
    pub const SIZE: usize = 1;

    /// Create new preferences with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create preferences with a specific power mode.
    pub fn with_power_mode(power_mode: PowerMode) -> Self {
        Self {
            power_mode,
            reserved: 0,
        }
    }

    /// Parse from a single byte.
    ///
    /// Format (8 bits):
    /// - Bits 0-1: Power mode (2 bits)
    /// - Bits 2-7: Reserved
    pub fn from_byte(byte: u8) -> Self {
        Self {
            power_mode: PowerMode::from_raw(byte & 0x03),
            reserved: (byte >> 2) & 0x3F,
        }
    }

    /// Encode to a single byte.
    pub fn to_byte(&self) -> u8 {
        (self.power_mode.to_raw() & 0x03) | ((self.reserved & 0x3F) << 2)
    }

    /// Check if the probe is in always-on mode.
    pub fn is_always_on(&self) -> bool {
        self.power_mode.is_always_on()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_mode_from_raw() {
        assert_eq!(PowerMode::from_raw(0), PowerMode::Normal);
        assert_eq!(PowerMode::from_raw(1), PowerMode::AlwaysOn);
        assert_eq!(PowerMode::from_raw(2), PowerMode::Normal); // Reserved
        assert_eq!(PowerMode::from_raw(3), PowerMode::Normal); // Reserved
        assert_eq!(PowerMode::from_raw(0xFF), PowerMode::Normal); // Only bottom 2 bits matter
    }

    #[test]
    fn test_power_mode_roundtrip() {
        for mode in [PowerMode::Normal, PowerMode::AlwaysOn] {
            let raw = mode.to_raw();
            let parsed = PowerMode::from_raw(raw);
            assert_eq!(mode, parsed);
        }
    }

    #[test]
    fn test_thermometer_preferences_roundtrip() {
        let prefs = ThermometerPreferences::with_power_mode(PowerMode::AlwaysOn);
        let byte = prefs.to_byte();
        let parsed = ThermometerPreferences::from_byte(byte);

        assert_eq!(prefs.power_mode, parsed.power_mode);
    }

    #[test]
    fn test_thermometer_preferences_parse() {
        // Byte 0b00000001 = AlwaysOn mode
        let prefs = ThermometerPreferences::from_byte(0x01);
        assert_eq!(prefs.power_mode, PowerMode::AlwaysOn);

        // Byte 0b00000000 = Normal mode
        let prefs = ThermometerPreferences::from_byte(0x00);
        assert_eq!(prefs.power_mode, PowerMode::Normal);

        // Byte with reserved bits set
        let prefs = ThermometerPreferences::from_byte(0xFD);
        assert_eq!(prefs.power_mode, PowerMode::AlwaysOn); // Bottom 2 bits = 01
    }

    #[test]
    fn test_power_mode_names() {
        assert_eq!(PowerMode::Normal.name(), "Normal");
        assert_eq!(PowerMode::AlwaysOn.name(), "Always On");
    }
}
