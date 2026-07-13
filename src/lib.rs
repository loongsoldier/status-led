#![no_std]
#![doc = "Monochrome status LED abstraction for the embassy ecosystem.\n\nProvides polarity via [`PolarityMode`], optional PWM brightness with\ngamma correction."]

mod led;

#[cfg(feature = "pwm")]
pub mod pwm;

#[cfg(feature = "breath")]
pub mod breath;

pub use led::{Led, PolarityMode};

#[cfg(feature = "pwm")]
pub use pwm::{GammaCorrection, GammaMap, PwmLed};

#[cfg(feature = "breath")]
pub use breath::Breath;

#[cfg(feature = "breath")]
pub use breath::BreathLed;
