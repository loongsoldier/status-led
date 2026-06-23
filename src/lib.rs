#![no_std]
#![doc = "Monochrome status LED abstraction for the embassy ecosystem.\n\nProvides type-safe compile-time polarity (`ActiveHigh` / `ActiveLow`),\noptional PWM brightness with gamma correction, and `FlexLed` /\n`FlexPwmLed` for runtime-determined polarity."]

mod led;
mod polarity;

#[cfg(feature = "pwm")]
pub mod pwm;

pub use led::{FlexLed, Led};
pub use polarity::{ActiveHigh, ActiveLow, Polarity, PolarityMode};

#[cfg(feature = "pwm")]
pub use pwm::{FlexPwmLed, GammaCorrection, PwmLed};
