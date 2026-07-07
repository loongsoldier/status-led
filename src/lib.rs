#![no_std]
#![doc = "Monochrome status LED abstraction for the embassy ecosystem.\n\nProvides polarity via [`PolarityMode`], optional PWM brightness with\ngamma correction."]

mod led;

#[cfg(feature = "pwm")]
pub mod pwm;

pub use led::{Led, PolarityMode};

#[cfg(feature = "pwm")]
pub use pwm::{GammaCorrection, PwmLed};
