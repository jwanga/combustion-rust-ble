//! Food safety data structures.
//!
//! Contains types for managing USDA food safety compliance monitoring.

/// Food product types for safety calculations.
///
/// Each product type has specific temperature and time requirements
/// for safe consumption based on USDA guidelines.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FoodSafeProduct {
    // Beef products
    /// Beef steak
    BeefSteak,
    /// Beef roast
    BeefRoast,
    /// Ground beef
    GroundBeef,

    // Pork products
    /// Pork chop
    PorkChop,
    /// Pork roast
    PorkRoast,
    /// Ground pork
    GroundPork,

    // Poultry
    /// Chicken breast
    ChickenBreast,
    /// Whole chicken
    ChickenWhole,
    /// Turkey
    Turkey,

    // Seafood
    /// Generic fish
    Fish,
    /// Salmon
    Salmon,

    /// Custom food safety parameters.
    Custom {
        /// Target log reduction (e.g., 6.5 for 6.5D reduction)
        log_reduction: f64,
        /// Reference temperature in Celsius
        reference_temp: f64,
        /// Z-value for the pathogen
        z_value: f64,
    },
}

impl Default for FoodSafeProduct {
    fn default() -> Self {
        Self::BeefSteak
    }
}

impl FoodSafeProduct {
    /// Get the default log reduction target for this product.
    pub fn default_log_reduction(&self) -> f64 {
        match self {
            // Poultry requires higher reduction due to Salmonella
            Self::ChickenBreast | Self::ChickenWhole | Self::Turkey => 7.0,
            // Ground products need higher reduction
            Self::GroundBeef | Self::GroundPork => 6.5,
            // Whole muscle beef/pork
            Self::BeefSteak | Self::BeefRoast | Self::PorkChop | Self::PorkRoast => 6.5,
            // Fish
            Self::Fish | Self::Salmon => 6.0,
            // Custom
            Self::Custom { log_reduction, .. } => *log_reduction,
        }
    }

    /// Get the reference temperature in Celsius for this product.
    pub fn reference_temperature(&self) -> f64 {
        match self {
            // Beef and pork use 70°C reference
            Self::BeefSteak
            | Self::BeefRoast
            | Self::GroundBeef
            | Self::PorkChop
            | Self::PorkRoast
            | Self::GroundPork => 70.0,
            // Poultry uses 74°C reference
            Self::ChickenBreast | Self::ChickenWhole | Self::Turkey => 74.0,
            // Fish uses lower temperature
            Self::Fish | Self::Salmon => 63.0,
            // Custom
            Self::Custom { reference_temp, .. } => *reference_temp,
        }
    }

    /// Get the Z-value for this product.
    ///
    /// The Z-value represents the temperature change needed to change
    /// the D-value by a factor of 10.
    pub fn z_value(&self) -> f64 {
        match self {
            // Most products use ~5.5°C z-value for Salmonella
            Self::BeefSteak
            | Self::BeefRoast
            | Self::GroundBeef
            | Self::PorkChop
            | Self::PorkRoast
            | Self::GroundPork
            | Self::ChickenBreast
            | Self::ChickenWhole
            | Self::Turkey => 5.5,
            // Fish
            Self::Fish | Self::Salmon => 6.0,
            // Custom
            Self::Custom { z_value, .. } => *z_value,
        }
    }

    /// Get the raw product type value for UART message.
    pub fn to_raw(&self) -> u8 {
        match self {
            Self::BeefSteak => 0,
            Self::BeefRoast => 1,
            Self::GroundBeef => 2,
            Self::PorkChop => 3,
            Self::PorkRoast => 4,
            Self::GroundPork => 5,
            Self::ChickenBreast => 6,
            Self::ChickenWhole => 7,
            Self::Turkey => 8,
            Self::Fish => 9,
            Self::Salmon => 10,
            Self::Custom { .. } => 255,
        }
    }

    /// Create from raw value.
    pub fn from_raw(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::BeefSteak),
            1 => Some(Self::BeefRoast),
            2 => Some(Self::GroundBeef),
            3 => Some(Self::PorkChop),
            4 => Some(Self::PorkRoast),
            5 => Some(Self::GroundPork),
            6 => Some(Self::ChickenBreast),
            7 => Some(Self::ChickenWhole),
            8 => Some(Self::Turkey),
            9 => Some(Self::Fish),
            10 => Some(Self::Salmon),
            _ => None,
        }
    }
}

/// Food safety serving state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FoodSafeServingState {
    /// Food has not reached safe serving criteria.
    #[default]
    NotSafe,
    /// Food is safe to serve according to USDA guidelines.
    SafeToServe,
}

impl FoodSafeServingState {
    /// Create from raw value.
    pub fn from_raw(value: u8) -> Self {
        match value {
            1 => Self::SafeToServe,
            _ => Self::NotSafe,
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u8 {
        match self {
            Self::NotSafe => 0,
            Self::SafeToServe => 1,
        }
    }

    /// Check if food is safe to serve.
    pub fn is_safe(&self) -> bool {
        matches!(self, Self::SafeToServe)
    }
}

/// Complete food safety data from the probe.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FoodSafeData {
    /// The food product being monitored.
    pub product: FoodSafeProduct,

    /// Current serving state.
    pub serving_state: FoodSafeServingState,

    /// Current log reduction achieved.
    pub log_reduction: f64,

    /// Seconds the food has been above the minimum safe temperature.
    pub seconds_above_threshold: u32,
}

impl FoodSafeData {
    /// Create new food safety data for a product.
    pub fn new(product: FoodSafeProduct) -> Self {
        Self {
            product,
            serving_state: FoodSafeServingState::NotSafe,
            log_reduction: 0.0,
            seconds_above_threshold: 0,
        }
    }

    /// Get the progress towards safe serving as a percentage.
    pub fn progress_percent(&self) -> f64 {
        let target = self.product.default_log_reduction();
        if target <= 0.0 {
            return 100.0;
        }
        (self.log_reduction / target * 100.0).min(100.0)
    }

    /// Check if the food is safe to serve.
    pub fn is_safe(&self) -> bool {
        self.serving_state.is_safe()
    }

    /// Get the remaining log reduction needed.
    pub fn remaining_reduction(&self) -> f64 {
        let target = self.product.default_log_reduction();
        (target - self.log_reduction).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_food_safe_product_defaults() {
        assert_eq!(FoodSafeProduct::ChickenBreast.default_log_reduction(), 7.0);
        assert_eq!(FoodSafeProduct::BeefSteak.default_log_reduction(), 6.5);
        assert_eq!(FoodSafeProduct::ChickenBreast.reference_temperature(), 74.0);
        assert_eq!(FoodSafeProduct::BeefSteak.reference_temperature(), 70.0);
    }

    #[test]
    fn test_food_safe_product_custom() {
        let custom = FoodSafeProduct::Custom {
            log_reduction: 5.0,
            reference_temp: 65.0,
            z_value: 6.0,
        };
        assert_eq!(custom.default_log_reduction(), 5.0);
        assert_eq!(custom.reference_temperature(), 65.0);
        assert_eq!(custom.z_value(), 6.0);
    }

    #[test]
    fn test_food_safe_product_roundtrip() {
        for value in 0..=10 {
            if let Some(product) = FoodSafeProduct::from_raw(value) {
                assert_eq!(product.to_raw(), value);
            }
        }
    }

    #[test]
    fn test_food_safe_serving_state() {
        assert!(!FoodSafeServingState::NotSafe.is_safe());
        assert!(FoodSafeServingState::SafeToServe.is_safe());
        assert_eq!(
            FoodSafeServingState::from_raw(0),
            FoodSafeServingState::NotSafe
        );
        assert_eq!(
            FoodSafeServingState::from_raw(1),
            FoodSafeServingState::SafeToServe
        );
    }

    #[test]
    fn test_food_safe_data_progress() {
        let mut data = FoodSafeData::new(FoodSafeProduct::ChickenBreast);
        data.log_reduction = 3.5;
        assert!((data.progress_percent() - 50.0).abs() < 0.1);

        data.log_reduction = 7.0;
        assert!((data.progress_percent() - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_food_safe_data_remaining() {
        let mut data = FoodSafeData::new(FoodSafeProduct::BeefSteak);
        data.log_reduction = 4.0;
        assert!((data.remaining_reduction() - 2.5).abs() < 0.1);

        data.log_reduction = 10.0;
        assert_eq!(data.remaining_reduction(), 0.0);
    }
}
