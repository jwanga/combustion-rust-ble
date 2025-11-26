//! Utility functions for the combustion-rust-ble crate.

/// Convert Celsius to Fahrenheit.
///
/// # Arguments
///
/// * `celsius` - Temperature in degrees Celsius
///
/// # Returns
///
/// Temperature in degrees Fahrenheit
///
/// # Example
///
/// ```
/// use combustion_rust_ble::celsius_to_fahrenheit;
///
/// let fahrenheit = celsius_to_fahrenheit(100.0);
/// assert!((fahrenheit - 212.0).abs() < 0.001);
/// ```
#[inline]
pub fn celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

/// Convert Fahrenheit to Celsius.
///
/// # Arguments
///
/// * `fahrenheit` - Temperature in degrees Fahrenheit
///
/// # Returns
///
/// Temperature in degrees Celsius
///
/// # Example
///
/// ```
/// use combustion_rust_ble::fahrenheit_to_celsius;
///
/// let celsius = fahrenheit_to_celsius(212.0);
/// assert!((celsius - 100.0).abs() < 0.001);
/// ```
#[inline]
pub fn fahrenheit_to_celsius(fahrenheit: f64) -> f64 {
    (fahrenheit - 32.0) * 5.0 / 9.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_celsius_to_fahrenheit() {
        assert!((celsius_to_fahrenheit(0.0) - 32.0).abs() < 0.001);
        assert!((celsius_to_fahrenheit(100.0) - 212.0).abs() < 0.001);
        assert!((celsius_to_fahrenheit(-40.0) - (-40.0)).abs() < 0.001);
        assert!((celsius_to_fahrenheit(37.0) - 98.6).abs() < 0.001);
    }

    #[test]
    fn test_fahrenheit_to_celsius() {
        assert!((fahrenheit_to_celsius(32.0) - 0.0).abs() < 0.001);
        assert!((fahrenheit_to_celsius(212.0) - 100.0).abs() < 0.001);
        assert!((fahrenheit_to_celsius(-40.0) - (-40.0)).abs() < 0.001);
    }

    #[test]
    fn test_temperature_roundtrip() {
        let original = 63.5;
        let converted = fahrenheit_to_celsius(celsius_to_fahrenheit(original));
        assert!((converted - original).abs() < 0.0001);
    }
}
