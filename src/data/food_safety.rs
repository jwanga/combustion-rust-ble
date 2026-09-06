//! Food safety data structures.
//!
//! Contains types for managing USDA food safety compliance monitoring.
//! Based on the Combustion Probe BLE Specification for Food Safe Data.

/// Food Safe Mode - determines how safety calculations are performed.
///
/// 3-bit enumeration (bits 0-2 of Food Safe Data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum FoodSafeMode {
    /// Simplified mode - uses predefined USDA temperature thresholds.
    /// The product type determines the safety rules to follow.
    #[default]
    Simplified = 0,
    /// Integrated mode - uses time-temperature integration with custom parameters.
    /// Log reduction is calculated based on Z-value, D-value, and reference temperature.
    Integrated = 1,
}

impl FoodSafeMode {
    /// Create from raw value.
    pub fn from_raw(value: u8) -> Self {
        match value & 0x07 {
            0 => Self::Simplified,
            1 => Self::Integrated,
            _ => Self::Simplified, // Reserved values default to Simplified
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }
}

/// Simplified mode product types (10-bit enumeration).
///
/// These values are used by firmware to determine the food safety rules to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u16)]
pub enum SimplifiedProduct {
    /// Default product type.
    #[default]
    Default = 0,
    /// Any poultry (chicken, turkey, duck, etc.).
    AnyPoultry = 1,
    /// Beef cuts (steaks, roasts).
    BeefCuts = 2,
    /// Pork cuts (chops, roasts).
    PorkCuts = 3,
    /// Veal cuts.
    VealCuts = 4,
    /// Lamb cuts.
    LambCuts = 5,
    /// Ground meats (beef, pork, lamb, veal).
    GroundMeats = 6,
    /// Ham, fresh or smoked (uncooked).
    HamFreshOrSmoked = 7,
    /// Ham, cooked and reheated.
    HamCookedAndReheated = 8,
    /// Eggs.
    Eggs = 9,
    /// Fish and shellfish.
    FishAndShellfish = 10,
    /// Leftovers.
    Leftovers = 11,
    /// Casseroles.
    Casseroles = 12,
}

impl SimplifiedProduct {
    /// Create from raw value.
    pub fn from_raw(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::Default),
            1 => Some(Self::AnyPoultry),
            2 => Some(Self::BeefCuts),
            3 => Some(Self::PorkCuts),
            4 => Some(Self::VealCuts),
            5 => Some(Self::LambCuts),
            6 => Some(Self::GroundMeats),
            7 => Some(Self::HamFreshOrSmoked),
            8 => Some(Self::HamCookedAndReheated),
            9 => Some(Self::Eggs),
            10 => Some(Self::FishAndShellfish),
            11 => Some(Self::Leftovers),
            12 => Some(Self::Casseroles),
            _ => None,
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u16 {
        *self as u16
    }

    /// Get the safe temperature threshold in Celsius for this product.
    pub fn safe_temperature_celsius(&self) -> f64 {
        match self {
            Self::Default => 74.0,
            Self::AnyPoultry => 74.0,           // 165°F
            Self::BeefCuts => 63.0,             // 145°F + 3 min rest
            Self::PorkCuts => 63.0,             // 145°F + 3 min rest
            Self::VealCuts => 63.0,             // 145°F + 3 min rest
            Self::LambCuts => 63.0,             // 145°F + 3 min rest
            Self::GroundMeats => 71.0,          // 160°F
            Self::HamFreshOrSmoked => 63.0,     // 145°F + 3 min rest
            Self::HamCookedAndReheated => 74.0, // 165°F
            Self::Eggs => 71.0,                 // 160°F for egg dishes
            Self::FishAndShellfish => 63.0,     // 145°F
            Self::Leftovers => 74.0,            // 165°F
            Self::Casseroles => 74.0,           // 165°F
        }
    }
}

/// Integrated mode product types (10-bit enumeration).
///
/// For Integrated mode, while this value is stored in firmware, it's only for
/// sync purposes. The values are interpreted exclusively by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u16)]
pub enum IntegratedProduct {
    /// Poultry (default for integrated mode).
    #[default]
    Poultry = 0,
    /// Meats (whole muscle cuts).
    Meats = 1,
    /// Meats - ground, chopped, or stuffed.
    MeatsGroundChoppedOrStuffed = 2,
    /// Poultry - ground, chopped, or stuffed.
    PoultryGroundChoppedOrStuffed = 4,
    /// Seafood (whole).
    Seafood = 13,
    /// Seafood - ground or chopped.
    SeafoodGroundOrChopped = 14,
    /// Dairy - Milk (<10% fat).
    DairyMilk = 15,
    /// Other (generic category).
    Other = 16,
    /// Seafood - stuffed.
    SeafoodStuffed = 17,
    /// Eggs (whole).
    Eggs = 18,
    /// Egg yolk.
    EggsYolk = 19,
    /// Egg white.
    EggsWhite = 20,
    /// Dairy - Creams (>10% fat).
    DairyCreams = 21,
    /// Dairy - Ice Cream Mix, Eggnog.
    DairyIceCreamMixEggnog = 22,
    /// Custom product with user-defined parameters.
    Custom = 1023,
}

impl IntegratedProduct {
    /// Create from raw value.
    pub fn from_raw(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::Poultry),
            1 => Some(Self::Meats),
            2 => Some(Self::MeatsGroundChoppedOrStuffed),
            4 => Some(Self::PoultryGroundChoppedOrStuffed),
            13 => Some(Self::Seafood),
            14 => Some(Self::SeafoodGroundOrChopped),
            15 => Some(Self::DairyMilk),
            16 => Some(Self::Other),
            17 => Some(Self::SeafoodStuffed),
            18 => Some(Self::Eggs),
            19 => Some(Self::EggsYolk),
            20 => Some(Self::EggsWhite),
            21 => Some(Self::DairyCreams),
            22 => Some(Self::DairyIceCreamMixEggnog),
            1023 => Some(Self::Custom),
            _ => None,
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u16 {
        *self as u16
    }

    /// Get default Z-value for this product.
    pub fn default_z_value(&self) -> f64 {
        match self {
            Self::Poultry | Self::PoultryGroundChoppedOrStuffed => 5.5,
            Self::Meats | Self::MeatsGroundChoppedOrStuffed => 5.5,
            Self::Seafood | Self::SeafoodGroundOrChopped | Self::SeafoodStuffed => 6.0,
            Self::DairyMilk | Self::DairyCreams | Self::DairyIceCreamMixEggnog => 5.0,
            Self::Eggs | Self::EggsYolk | Self::EggsWhite => 4.5,
            Self::Other | Self::Custom => 5.5, // Safe default
        }
    }

    /// Get default reference temperature in Celsius.
    pub fn default_reference_temperature(&self) -> f64 {
        match self {
            Self::Poultry | Self::PoultryGroundChoppedOrStuffed => 70.0,
            Self::Meats | Self::MeatsGroundChoppedOrStuffed => 70.0,
            Self::Seafood | Self::SeafoodGroundOrChopped | Self::SeafoodStuffed => 65.0,
            Self::DairyMilk | Self::DairyCreams | Self::DairyIceCreamMixEggnog => 72.0,
            Self::Eggs | Self::EggsYolk | Self::EggsWhite => 70.0,
            Self::Other | Self::Custom => 70.0,
        }
    }

    /// Get default D-value at reference temperature (in seconds).
    pub fn default_d_value(&self) -> f64 {
        match self {
            Self::Poultry | Self::PoultryGroundChoppedOrStuffed => 1.0,
            Self::Meats | Self::MeatsGroundChoppedOrStuffed => 5.0,
            Self::Seafood | Self::SeafoodGroundOrChopped | Self::SeafoodStuffed => 1.0,
            Self::DairyMilk | Self::DairyCreams | Self::DairyIceCreamMixEggnog => 0.5,
            Self::Eggs | Self::EggsYolk | Self::EggsWhite => 0.6,
            Self::Other | Self::Custom => 5.0,
        }
    }

    /// Get default target log reduction.
    pub fn default_target_log_reduction(&self) -> f64 {
        match self {
            Self::Poultry | Self::PoultryGroundChoppedOrStuffed => 7.0,
            Self::Meats => 5.0,
            Self::MeatsGroundChoppedOrStuffed => 6.5,
            Self::Seafood | Self::SeafoodGroundOrChopped | Self::SeafoodStuffed => 6.0,
            Self::DairyMilk | Self::DairyCreams | Self::DairyIceCreamMixEggnog => 5.0,
            Self::Eggs | Self::EggsYolk | Self::EggsWhite => 5.0,
            Self::Other | Self::Custom => 6.5,
        }
    }
}

/// Serving mode - how the food will be served after cooking.
///
/// 3-bit enumeration (bits 13-15 of Food Safe Data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Serving {
    /// Food will be served immediately after cooking.
    #[default]
    ServedImmediately = 0,
    /// Food will be cooked and then chilled for later use.
    CookedAndChilled = 1,
}

impl Serving {
    /// Create from raw value.
    pub fn from_raw(value: u8) -> Self {
        match value & 0x07 {
            0 => Self::ServedImmediately,
            1 => Self::CookedAndChilled,
            _ => Self::ServedImmediately, // Reserved values default
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }
}

/// Food Safe State - current state of the food safe program.
///
/// 3-bit enumeration from Food Safe Status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum FoodSafeState {
    /// Food has not reached safe serving criteria.
    #[default]
    NotSafe = 0,
    /// Food is safe to serve according to USDA guidelines.
    Safe = 1,
    /// Safety is impossible to achieve (e.g., temperature went below threshold).
    SafetyImpossible = 2,
}

impl FoodSafeState {
    /// Create from raw value.
    pub fn from_raw(value: u8) -> Self {
        match value & 0x07 {
            0 => Self::NotSafe,
            1 => Self::Safe,
            2 => Self::SafetyImpossible,
            _ => Self::NotSafe, // Reserved values
        }
    }

    /// Convert to raw value.
    pub fn to_raw(&self) -> u8 {
        *self as u8
    }

    /// Check if food is safe to serve.
    pub fn is_safe(&self) -> bool {
        matches!(self, Self::Safe)
    }

    /// Check if safety is still achievable.
    pub fn is_achievable(&self) -> bool {
        !matches!(self, Self::SafetyImpossible)
    }
}

/// Configuration parameters for the Food Safe feature.
///
/// This is a packed 10-byte (80-bit) structure sent to configure food safety monitoring.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FoodSafeConfig {
    /// Food safe mode (Simplified or Integrated).
    pub mode: FoodSafeMode,
    /// Product type (interpretation depends on mode).
    pub product: u16,
    /// Serving mode.
    pub serving: Serving,
    /// Selected threshold reference temperature in Celsius.
    pub threshold_temperature: f64,
    /// Z-value for the pathogen (temperature change to reduce D-value by 10x).
    pub z_value: f64,
    /// Reference temperature in Celsius for D-value.
    pub reference_temperature: f64,
    /// D-value at reference temperature (time to reduce population by 90%).
    pub d_value_at_reference: f64,
    /// Target log reduction to achieve.
    pub target_log_reduction: f64,
}

impl Default for FoodSafeConfig {
    fn default() -> Self {
        Self {
            mode: FoodSafeMode::Simplified,
            product: 0,
            serving: Serving::ServedImmediately,
            threshold_temperature: 54.4, // ~130°F - typical minimum for pathogen reduction
            z_value: 5.5,
            reference_temperature: 70.0,
            d_value_at_reference: 5.0,
            target_log_reduction: 6.5,
        }
    }
}

impl FoodSafeConfig {
    /// Create a simplified mode configuration for a product.
    pub fn simplified(product: SimplifiedProduct, serving: Serving) -> Self {
        Self {
            mode: FoodSafeMode::Simplified,
            product: product.to_raw(),
            serving,
            threshold_temperature: product.safe_temperature_celsius(),
            z_value: 5.5,
            reference_temperature: 70.0,
            d_value_at_reference: 5.0,
            target_log_reduction: 6.5,
        }
    }

    /// Create an integrated mode configuration for a product.
    pub fn integrated(product: IntegratedProduct, serving: Serving) -> Self {
        Self {
            mode: FoodSafeMode::Integrated,
            product: product.to_raw(),
            serving,
            threshold_temperature: 54.4, // ~130°F threshold for integration
            z_value: product.default_z_value(),
            reference_temperature: product.default_reference_temperature(),
            d_value_at_reference: product.default_d_value(),
            target_log_reduction: product.default_target_log_reduction(),
        }
    }

    /// Create a custom integrated mode configuration.
    pub fn custom(
        threshold_temperature: f64,
        z_value: f64,
        reference_temperature: f64,
        d_value_at_reference: f64,
        target_log_reduction: f64,
        serving: Serving,
    ) -> Self {
        Self {
            mode: FoodSafeMode::Integrated,
            product: IntegratedProduct::Custom.to_raw(),
            serving,
            threshold_temperature,
            z_value,
            reference_temperature,
            d_value_at_reference,
            target_log_reduction,
        }
    }

    /// Encode to 10-byte packed format for BLE transmission.
    ///
    /// Layout (80 bits):
    /// - Bits 0-2: Food Safe Mode (3 bits)
    /// - Bits 3-12: Product (10 bits)
    /// - Bits 13-15: Serving (3 bits)
    /// - Bits 16-28: Threshold Temperature (13 bits, value / 0.05)
    /// - Bits 29-41: Z-value (13 bits, value / 0.05)
    /// - Bits 42-54: Reference Temperature (13 bits, value / 0.05)
    /// - Bits 55-67: D-value at RT (13 bits, value / 0.05)
    /// - Bits 68-75: Target Log Reduction (8 bits, value / 0.1)
    pub fn to_bytes(&self) -> [u8; 10] {
        let mut bytes = [0u8; 10];

        // Helper to encode temperature/value as 13-bit with 0.05 resolution
        let encode_13bit = |value: f64| -> u16 { (value / 0.05).round() as u16 & 0x1FFF };

        // Helper to encode log reduction as 8-bit with 0.1 resolution
        let encode_8bit = |value: f64| -> u8 { (value / 0.1).round() as u8 };

        // Byte 0: Mode (bits 0-2), Product low bits (bits 3-7)
        bytes[0] = (self.mode.to_raw() & 0x07) | ((self.product as u8 & 0x1F) << 3);

        // Byte 1: Product high bits (bits 0-4), Serving (bits 5-7)
        bytes[1] = ((self.product >> 5) as u8 & 0x1F) | ((self.serving.to_raw() & 0x07) << 5);

        // Bytes 2-3: Threshold Temperature (13 bits starting at bit 16)
        let threshold = encode_13bit(self.threshold_temperature);
        bytes[2] = threshold as u8;
        bytes[3] = ((threshold >> 8) & 0x1F) as u8;

        // Bytes 3-4: Z-value (13 bits starting at bit 29)
        let z = encode_13bit(self.z_value);
        bytes[3] |= ((z & 0x07) << 5) as u8;
        bytes[4] = ((z >> 3) & 0xFF) as u8;
        bytes[5] = ((z >> 11) & 0x03) as u8;

        // Bytes 5-6: Reference Temperature (13 bits starting at bit 42)
        let ref_temp = encode_13bit(self.reference_temperature);
        bytes[5] |= ((ref_temp & 0x3F) << 2) as u8;
        bytes[6] = ((ref_temp >> 6) & 0x7F) as u8;

        // Bytes 6-8: D-value (13 bits starting at bit 55)
        let d = encode_13bit(self.d_value_at_reference);
        bytes[6] |= ((d & 0x01) << 7) as u8;
        bytes[7] = ((d >> 1) & 0xFF) as u8;
        bytes[8] = ((d >> 9) & 0x0F) as u8;

        // Bytes 8-9: Target Log Reduction (8 bits starting at bit 68)
        let log_red = encode_8bit(self.target_log_reduction);
        bytes[8] |= ((log_red & 0x0F) << 4) as u8;
        bytes[9] = ((log_red >> 4) & 0x0F) as u8;

        bytes
    }

    /// Decode from 10-byte packed format.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 {
            return None;
        }

        // Helper to decode 13-bit value with 0.05 resolution
        let decode_13bit = |raw: u16| -> f64 { (raw & 0x1FFF) as f64 * 0.05 };

        // Helper to decode 8-bit value with 0.1 resolution
        let decode_8bit = |raw: u8| -> f64 { raw as f64 * 0.1 };

        // Byte 0: Mode (bits 0-2), Product low bits (bits 3-7)
        let mode = FoodSafeMode::from_raw(bytes[0] & 0x07);
        let product_low = (bytes[0] >> 3) as u16;

        // Byte 1: Product high bits (bits 0-4), Serving (bits 5-7)
        let product_high = (bytes[1] & 0x1F) as u16;
        let product = product_low | (product_high << 5);
        let serving = Serving::from_raw((bytes[1] >> 5) & 0x07);

        // Bytes 2-3: Threshold Temperature (13 bits starting at bit 16)
        let threshold_raw = bytes[2] as u16 | ((bytes[3] & 0x1F) as u16) << 8;
        let threshold_temperature = decode_13bit(threshold_raw);

        // Bytes 3-5: Z-value (13 bits starting at bit 29)
        let z_raw =
            ((bytes[3] >> 5) as u16) | ((bytes[4] as u16) << 3) | ((bytes[5] & 0x03) as u16) << 11;
        let z_value = decode_13bit(z_raw);

        // Bytes 5-6: Reference Temperature (13 bits starting at bit 42)
        let ref_raw = ((bytes[5] >> 2) as u16) | ((bytes[6] & 0x7F) as u16) << 6;
        let reference_temperature = decode_13bit(ref_raw);

        // Bytes 6-8: D-value (13 bits starting at bit 55)
        let d_raw =
            ((bytes[6] >> 7) as u16) | ((bytes[7] as u16) << 1) | ((bytes[8] & 0x0F) as u16) << 9;
        let d_value_at_reference = decode_13bit(d_raw);

        // Bytes 8-9: Target Log Reduction (8 bits starting at bit 68)
        let log_raw = ((bytes[8] >> 4) as u8) | ((bytes[9] & 0x0F) << 4);
        let target_log_reduction = decode_8bit(log_raw);

        Some(Self {
            mode,
            product,
            serving,
            threshold_temperature,
            z_value,
            reference_temperature,
            d_value_at_reference,
            target_log_reduction,
        })
    }
}

/// Food Safe Status - current status of the food safe program.
///
/// This is parsed from an 8-byte packed structure in probe status notifications.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FoodSafeStatus {
    /// Current state of the food safe program.
    pub state: FoodSafeState,
    /// Current log reduction achieved (0.0 to 25.5 in 0.1 steps).
    /// In Simplified mode, this is always 0.
    pub log_reduction: f64,
    /// Seconds the core temperature has been above the threshold.
    pub seconds_above_threshold: u32,
    /// Sequence number of the log entry when food safe was started.
    pub sequence_number: u32,
}

impl FoodSafeStatus {
    /// Parse from 8-byte packed format.
    ///
    /// Layout (64 bits):
    /// - Bits 0-2: Food Safe State (3 bits)
    /// - Bits 3-10: Log Reduction (8 bits, value * 0.1)
    /// - Bits 11-26: Seconds above threshold (16 bits)
    /// - Bits 27-58: Sequence number (32 bits)
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }

        // Bits 0-2: State
        let state = FoodSafeState::from_raw(bytes[0] & 0x07);

        // Bits 3-10: Log Reduction (8 bits)
        let log_raw = ((bytes[0] >> 3) as u8) | ((bytes[1] & 0x07) << 5);
        let log_reduction = log_raw as f64 * 0.1;

        // Bits 11-26: Seconds above threshold (16 bits)
        let seconds_raw = ((bytes[1] >> 3) as u16)
            | ((bytes[2] as u16) << 5)
            | ((bytes[3] & 0x07) as u16) << 13;
        let seconds_above_threshold = seconds_raw as u32;

        // Bits 27-58: Sequence number (32 bits)
        let seq_raw = ((bytes[3] >> 3) as u32)
            | ((bytes[4] as u32) << 5)
            | ((bytes[5] as u32) << 13)
            | ((bytes[6] as u32) << 21)
            | ((bytes[7] & 0x07) as u32) << 29;
        let sequence_number = seq_raw;

        Some(Self {
            state,
            log_reduction,
            seconds_above_threshold,
            sequence_number,
        })
    }

    /// Check if food is safe to serve.
    pub fn is_safe(&self) -> bool {
        self.state.is_safe()
    }

    /// Check if safety is still achievable.
    pub fn is_achievable(&self) -> bool {
        self.state.is_achievable()
    }
}

// === Legacy compatibility types ===

/// Food product types for safety calculations (legacy compatibility).
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

    /// Convert to SimplifiedProduct for firmware configuration.
    pub fn to_simplified(&self) -> SimplifiedProduct {
        match self {
            Self::ChickenBreast | Self::ChickenWhole | Self::Turkey => SimplifiedProduct::AnyPoultry,
            Self::BeefSteak | Self::BeefRoast => SimplifiedProduct::BeefCuts,
            Self::PorkChop | Self::PorkRoast => SimplifiedProduct::PorkCuts,
            Self::GroundBeef | Self::GroundPork => SimplifiedProduct::GroundMeats,
            Self::Fish | Self::Salmon => SimplifiedProduct::FishAndShellfish,
            Self::Custom { .. } => SimplifiedProduct::Default,
        }
    }

    /// Convert to IntegratedProduct for firmware configuration.
    pub fn to_integrated(&self) -> IntegratedProduct {
        match self {
            Self::ChickenBreast | Self::ChickenWhole | Self::Turkey => IntegratedProduct::Poultry,
            Self::BeefSteak | Self::BeefRoast | Self::PorkChop | Self::PorkRoast => {
                IntegratedProduct::Meats
            }
            Self::GroundBeef | Self::GroundPork => IntegratedProduct::MeatsGroundChoppedOrStuffed,
            Self::Fish | Self::Salmon => IntegratedProduct::Seafood,
            Self::Custom { .. } => IntegratedProduct::Custom,
        }
    }

    /// Get the raw product type value for UART message (simplified mode).
    pub fn to_raw(&self) -> u8 {
        self.to_simplified().to_raw() as u8
    }

    /// Create FoodSafeConfig for this product in simplified mode.
    pub fn to_config(&self, serving: Serving) -> FoodSafeConfig {
        FoodSafeConfig::simplified(self.to_simplified(), serving)
    }

    /// Create FoodSafeConfig for this product in integrated mode.
    pub fn to_integrated_config(&self, serving: Serving) -> FoodSafeConfig {
        match self {
            Self::Custom {
                log_reduction,
                reference_temp,
                z_value,
            } => FoodSafeConfig::custom(
                54.4, // Standard threshold for integration
                *z_value,
                *reference_temp,
                5.0, // Default D-value
                *log_reduction,
                serving,
            ),
            _ => FoodSafeConfig::integrated(self.to_integrated(), serving),
        }
    }

    /// Create from raw value.
    pub fn from_raw(value: u8) -> Option<Self> {
        SimplifiedProduct::from_raw(value as u16).map(|p| match p {
            SimplifiedProduct::Default => Self::BeefSteak,
            SimplifiedProduct::AnyPoultry => Self::ChickenBreast,
            SimplifiedProduct::BeefCuts => Self::BeefSteak,
            SimplifiedProduct::PorkCuts => Self::PorkChop,
            SimplifiedProduct::VealCuts => Self::BeefSteak,
            SimplifiedProduct::LambCuts => Self::BeefSteak,
            SimplifiedProduct::GroundMeats => Self::GroundBeef,
            SimplifiedProduct::HamFreshOrSmoked => Self::PorkRoast,
            SimplifiedProduct::HamCookedAndReheated => Self::PorkRoast,
            SimplifiedProduct::Eggs => Self::ChickenBreast, // No direct mapping
            SimplifiedProduct::FishAndShellfish => Self::Fish,
            SimplifiedProduct::Leftovers => Self::ChickenBreast, // Safe default
            SimplifiedProduct::Casseroles => Self::ChickenBreast, // Safe default
        })
    }
}

/// Food safety serving state (legacy compatibility).
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
    /// Create from FoodSafeState.
    pub fn from_state(state: FoodSafeState) -> Self {
        match state {
            FoodSafeState::Safe => Self::SafeToServe,
            _ => Self::NotSafe,
        }
    }

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
    /// The food product being monitored (legacy).
    pub product: FoodSafeProduct,

    /// Current serving state (legacy).
    pub serving_state: FoodSafeServingState,

    /// Current log reduction achieved.
    pub log_reduction: f64,

    /// Seconds the food has been above the minimum safe temperature.
    pub seconds_above_threshold: u32,

    /// Configuration sent to the probe.
    pub config: Option<FoodSafeConfig>,

    /// Current status from the probe.
    pub status: Option<FoodSafeStatus>,
}

impl FoodSafeData {
    /// Create new food safety data for a product (legacy compatibility).
    pub fn new(product: FoodSafeProduct) -> Self {
        Self {
            product,
            serving_state: FoodSafeServingState::NotSafe,
            log_reduction: 0.0,
            seconds_above_threshold: 0,
            config: None,
            status: None,
        }
    }

    /// Create new food safety data with configuration.
    pub fn with_config(config: FoodSafeConfig) -> Self {
        Self {
            product: FoodSafeProduct::default(),
            serving_state: FoodSafeServingState::NotSafe,
            log_reduction: 0.0,
            seconds_above_threshold: 0,
            config: Some(config),
            status: None,
        }
    }

    /// Create food safety data from config and status (for external updates).
    pub fn from_config_and_status(config: FoodSafeConfig, status: FoodSafeStatus) -> Self {
        let serving_state = FoodSafeServingState::from_state(status.state);
        Self {
            product: FoodSafeProduct::default(),
            serving_state,
            log_reduction: status.log_reduction,
            seconds_above_threshold: status.seconds_above_threshold,
            config: Some(config),
            status: Some(status),
        }
    }

    /// Update the configuration (e.g., when changed externally).
    pub fn update_config(&mut self, config: FoodSafeConfig) {
        self.config = Some(config);
    }

    /// Update from status notification.
    pub fn update_from_status(&mut self, status: FoodSafeStatus) {
        self.serving_state = FoodSafeServingState::from_state(status.state);
        self.log_reduction = status.log_reduction;
        self.seconds_above_threshold = status.seconds_above_threshold;
        self.status = Some(status);
    }

    /// Get the progress towards safe serving as a percentage.
    pub fn progress_percent(&self) -> f64 {
        let target = self
            .config
            .as_ref()
            .map(|c| c.target_log_reduction)
            .unwrap_or_else(|| self.product.default_log_reduction());
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
        let target = self
            .config
            .as_ref()
            .map(|c| c.target_log_reduction)
            .unwrap_or_else(|| self.product.default_log_reduction());
        (target - self.log_reduction).max(0.0)
    }

    /// Get the current food safe state.
    pub fn state(&self) -> FoodSafeState {
        self.status
            .as_ref()
            .map(|s| s.state)
            .unwrap_or(FoodSafeState::NotSafe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_food_safe_mode() {
        assert_eq!(FoodSafeMode::from_raw(0), FoodSafeMode::Simplified);
        assert_eq!(FoodSafeMode::from_raw(1), FoodSafeMode::Integrated);
        assert_eq!(FoodSafeMode::from_raw(7), FoodSafeMode::Simplified); // Reserved
    }

    #[test]
    fn test_simplified_product() {
        assert_eq!(
            SimplifiedProduct::from_raw(1),
            Some(SimplifiedProduct::AnyPoultry)
        );
        assert_eq!(
            SimplifiedProduct::from_raw(6),
            Some(SimplifiedProduct::GroundMeats)
        );
        assert_eq!(SimplifiedProduct::from_raw(100), None);

        // Check safe temperatures
        assert_eq!(SimplifiedProduct::AnyPoultry.safe_temperature_celsius(), 74.0);
        assert_eq!(SimplifiedProduct::BeefCuts.safe_temperature_celsius(), 63.0);
        assert_eq!(SimplifiedProduct::GroundMeats.safe_temperature_celsius(), 71.0);
    }

    #[test]
    fn test_integrated_product() {
        assert_eq!(
            IntegratedProduct::from_raw(0),
            Some(IntegratedProduct::Poultry)
        );
        assert_eq!(
            IntegratedProduct::from_raw(1023),
            Some(IntegratedProduct::Custom)
        );
        assert_eq!(IntegratedProduct::from_raw(3), None); // Deprecated/missing

        // Check defaults
        assert_eq!(IntegratedProduct::Poultry.default_z_value(), 5.5);
        assert_eq!(IntegratedProduct::Seafood.default_z_value(), 6.0);
    }

    #[test]
    fn test_serving() {
        assert_eq!(Serving::from_raw(0), Serving::ServedImmediately);
        assert_eq!(Serving::from_raw(1), Serving::CookedAndChilled);
        assert_eq!(Serving::from_raw(5), Serving::ServedImmediately); // Reserved
    }

    #[test]
    fn test_food_safe_state() {
        assert_eq!(FoodSafeState::from_raw(0), FoodSafeState::NotSafe);
        assert_eq!(FoodSafeState::from_raw(1), FoodSafeState::Safe);
        assert_eq!(FoodSafeState::from_raw(2), FoodSafeState::SafetyImpossible);

        assert!(!FoodSafeState::NotSafe.is_safe());
        assert!(FoodSafeState::Safe.is_safe());
        assert!(!FoodSafeState::SafetyImpossible.is_safe());

        assert!(FoodSafeState::NotSafe.is_achievable());
        assert!(FoodSafeState::Safe.is_achievable());
        assert!(!FoodSafeState::SafetyImpossible.is_achievable());
    }

    #[test]
    fn test_food_safe_config_roundtrip() {
        let config = FoodSafeConfig {
            mode: FoodSafeMode::Integrated,
            product: IntegratedProduct::Poultry.to_raw(),
            serving: Serving::ServedImmediately,
            threshold_temperature: 54.5,
            z_value: 5.5,
            reference_temperature: 70.0,
            d_value_at_reference: 1.0,
            target_log_reduction: 7.0,
        };

        let bytes = config.to_bytes();
        let parsed = FoodSafeConfig::from_bytes(&bytes).expect("should parse");

        assert_eq!(parsed.mode, config.mode);
        assert_eq!(parsed.product, config.product);
        assert_eq!(parsed.serving, config.serving);
        // Allow small floating point differences due to encoding resolution
        assert!((parsed.threshold_temperature - config.threshold_temperature).abs() < 0.1);
        assert!((parsed.z_value - config.z_value).abs() < 0.1);
        assert!((parsed.reference_temperature - config.reference_temperature).abs() < 0.1);
        assert!((parsed.d_value_at_reference - config.d_value_at_reference).abs() < 0.1);
        assert!((parsed.target_log_reduction - config.target_log_reduction).abs() < 0.2);
    }

    #[test]
    fn test_food_safe_config_simplified() {
        let config = FoodSafeConfig::simplified(SimplifiedProduct::AnyPoultry, Serving::ServedImmediately);
        assert_eq!(config.mode, FoodSafeMode::Simplified);
        assert_eq!(config.product, SimplifiedProduct::AnyPoultry.to_raw());
        assert_eq!(config.threshold_temperature, 74.0); // Poultry safe temp
    }

    #[test]
    fn test_food_safe_status_parse() {
        // Create test data with known values
        let mut bytes = [0u8; 8];
        // State = Safe (1), bits 0-2
        // Log reduction = 70 (7.0), bits 3-10
        // Combined: 0b_0111000_001 = 0x1C1 in first two bytes
        bytes[0] = 0b00111001; // State=1 (bits 0-2), log_red low bits (bits 3-7)
        bytes[1] = 0b00000001; // log_red high bits (bits 0-2), seconds low (bits 3-7)
        // Rest zeroed for simplicity

        let status = FoodSafeStatus::from_bytes(&bytes).expect("should parse");
        assert_eq!(status.state, FoodSafeState::Safe);
        assert!(status.is_safe());
        // Log reduction: bits 3-10 = (0b00111 from byte 0) | (0b001 << 5 from byte 1) = 7 + 32 = 39
        // Actually: (bytes[0] >> 3) | ((bytes[1] & 0x07) << 5) = 7 | (1 << 5) = 7 | 32 = 39
        // 39 * 0.1 = 3.9
        assert!((status.log_reduction - 3.9).abs() < 0.01);
    }

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
