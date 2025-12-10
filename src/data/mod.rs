//! Data structures for probe data.
//!
//! This module contains all the core data types used to represent
//! temperature data, predictions, sessions, food safety information,
//! alarms, and thermometer preferences.

pub mod alarms;
pub mod food_safety;
pub mod log;
pub mod prediction;
pub mod preferences;
pub mod session;
pub mod temperatures;

pub use alarms::{AlarmConfig, AlarmStatus, ALARM_ARRAY_SIZE, ALARM_COUNT};
pub use food_safety::{
    FoodSafeConfig, FoodSafeData, FoodSafeMode, FoodSafeProduct, FoodSafeServingState,
    FoodSafeState, FoodSafeStatus, IntegratedProduct, Serving, SimplifiedProduct,
};
pub use log::{LoggedDataPoint, PredictionLog, TemperatureLog};
pub use prediction::{PredictionInfo, PredictionMode, PredictionState, PredictionType};
pub use preferences::{PowerMode, ThermometerPreferences};
pub use session::SessionInfo;
pub use temperatures::{
    ProbeTemperatures, RawTemperature, VirtualSensorSelection, VirtualTemperatures,
};
