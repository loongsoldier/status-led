use embedded_hal::digital::PinState;

mod sealed {
    pub trait Sealed {}
}

/// Compile-time logic-level polarity: maps logical ON/OFF to physical High/Low.
///
/// This trait is sealed; only the built-in [`ActiveHigh`] / [`ActiveLow`] markers
/// implement it.  For runtime polarity use [`PolarityMode`] with [`FlexLed`] or
/// [`FlexPwmLed`].
///
/// [`FlexLed`]: crate::FlexLed
/// [`FlexPwmLed`]: crate::pwm::FlexPwmLed
pub trait Polarity: sealed::Sealed {
    /// Physical pin level corresponding to logical ON (GPIO).
    fn physical_on() -> PinState;
    /// Physical pin level corresponding to logical OFF (GPIO).
    fn physical_off() -> PinState;
    /// Given a physical high level, return whether the LED is logically ON (GPIO).
    fn is_logical_on(physical_high: bool) -> bool;
    /// Map a gamma-corrected on-time fraction (0=off, 255=fully on) to a PWM duty
    /// fraction (0=0% duty, 255=100% duty).
    fn map_duty(brightness: u8) -> u8;
    /// The corresponding runtime [`PolarityMode`].
    const MODE: PolarityMode;
}

/// Active-high: LED is on when the pin is High (most common).
pub struct ActiveHigh;
impl sealed::Sealed for ActiveHigh {}
impl Polarity for ActiveHigh {
    const MODE: PolarityMode = PolarityMode::ActiveHigh;
    #[inline]
    fn physical_on() -> PinState {
        PinState::High
    }
    #[inline]
    fn physical_off() -> PinState {
        PinState::Low
    }
    #[inline]
    fn is_logical_on(physical_high: bool) -> bool {
        physical_high
    }
    #[inline]
    fn map_duty(brightness: u8) -> u8 {
        brightness
    }
}

/// Active-low: LED is on when the pin is Low (common on Nucleo, ESP32 boards).
pub struct ActiveLow;
impl sealed::Sealed for ActiveLow {}
impl Polarity for ActiveLow {
    const MODE: PolarityMode = PolarityMode::ActiveLow;
    #[inline]
    fn physical_on() -> PinState {
        PinState::Low
    }
    #[inline]
    fn physical_off() -> PinState {
        PinState::High
    }
    #[inline]
    fn is_logical_on(physical_high: bool) -> bool {
        !physical_high
    }
    #[inline]
    fn map_duty(brightness: u8) -> u8 {
        255 - brightness
    }
}

// ─── Runtime polarity ─────────────────────────────────

/// Runtime polarity mode, for use with [`FlexLed`] / [`FlexPwmLed`].
///
/// Unlike the compile-time [`ActiveHigh`] / [`ActiveLow`] markers, this enum is
/// suitable when the polarity is determined at runtime (e.g. from configuration).
///
/// [`FlexLed`]: crate::FlexLed
/// [`FlexPwmLed`]: crate::pwm::FlexPwmLed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PolarityMode {
    /// Active-high: pin High = LED on.
    ActiveHigh,
    /// Active-low: pin Low = LED on.
    ActiveLow,
}

impl PolarityMode {
    #[inline]
    pub(crate) fn physical_on(self) -> PinState {
        match self {
            Self::ActiveHigh => PinState::High,
            Self::ActiveLow => PinState::Low,
        }
    }

    #[inline]
    pub(crate) fn physical_off(self) -> PinState {
        match self {
            Self::ActiveHigh => PinState::Low,
            Self::ActiveLow => PinState::High,
        }
    }

    #[inline]
    pub(crate) fn is_logical_on(self, physical_high: bool) -> bool {
        match self {
            Self::ActiveHigh => physical_high,
            Self::ActiveLow => !physical_high,
        }
    }

    #[inline]
    #[allow(dead_code)] // only used when feature = "pwm"
    pub(crate) fn map_duty(self, brightness: u8) -> u8 {
        match self {
            Self::ActiveHigh => brightness,
            Self::ActiveLow => 255 - brightness,
        }
    }
}
