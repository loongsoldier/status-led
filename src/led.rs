use embedded_hal::digital::{OutputPin, PinState, StatefulOutputPin};

/// Runtime polarity mode.
///
/// Determines whether a logical ON maps to a physical `High` or `Low` pin level.
/// Chosen at construction time, making it suitable for polarity derived from
/// runtime configuration.
///
/// # Examples
///
/// ```ignore
/// use status_led::{Led, PolarityMode};
///
/// let pol = if config.active_low {
///     PolarityMode::ActiveLow
/// } else {
///     PolarityMode::ActiveHigh
/// };
/// let mut led = Led::new(pin, pol).unwrap();
/// led.on().unwrap();
/// ```
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

// ─── Led ────────────────────────────────────────────

/// Monochrome LED with polarity chosen at construction time.
///
/// Stores a [`PolarityMode`] value — each operation incurs one `match` which
/// is negligible on Cortex-M.
///
/// No internal state cache — `is_on()` reads the hardware register (ODR) directly
/// and applies the polarity conversion.  In the embassy ecosystem `OutputPin::Error`
/// is [`Infallible`], so unwrapping results is safe.
///
/// # Examples
///
/// ```ignore
/// use status_led::{Led, PolarityMode};
///
/// let mut led = Led::new(pin, PolarityMode::ActiveLow).unwrap();
/// led.on().unwrap();
/// led.toggle().unwrap();
/// assert!(led.is_on().unwrap());
/// ```
///
/// [`Infallible`]: core::convert::Infallible
pub struct Led<P> {
    pin: P,
    polarity: PolarityMode,
}

impl<P> Led<P> {
    /// Build from an already-configured pin without changing its level.
    ///
    /// Prefer this when the HAL already set the pin to a known state
    /// (e.g. `Output::new(pin, Level::High, Speed::Low)` for an active-low LED).
    #[inline]
    pub fn from_pin(pin: P, polarity: PolarityMode) -> Self {
        Self { pin, polarity }
    }
}

impl<P: OutputPin> Led<P> {
    /// Build and force the pin to the logical OFF state.
    ///
    /// Safest default — guarantees the LED starts dark regardless of the pin's
    /// reset state.
    pub fn new(mut pin: P, polarity: PolarityMode) -> Result<Self, P::Error> {
        pin.set_state(polarity.physical_off())?;
        Ok(Self { pin, polarity })
    }

    /// Turn the LED on (logical ON → physical level determined by polarity).
    #[inline]
    pub fn on(&mut self) -> Result<(), P::Error> {
        self.pin.set_state(self.polarity.physical_on())
    }

    /// Turn the LED off (logical OFF → physical level determined by polarity).
    #[inline]
    pub fn off(&mut self) -> Result<(), P::Error> {
        self.pin.set_state(self.polarity.physical_off())
    }

    /// Set the logical state: `true` = ON, `false` = OFF.
    #[inline]
    pub fn set(&mut self, state: bool) -> Result<(), P::Error> {
        if state { self.on() } else { self.off() }
    }
}

impl<P: StatefulOutputPin> Led<P> {
    /// Read the logical state from the hardware register.
    ///
    /// Reads the physical pin level (ODR register on STM32), then converts
    /// through the polarity.  Requires `&mut self` because
    /// [`StatefulOutputPin::is_set_high`] does — in practice the read is a
    /// single register access with no side effects.
    #[inline]
    pub fn is_on(&mut self) -> Result<bool, P::Error> {
        self.pin
            .is_set_high()
            .map(|h| self.polarity.is_logical_on(h))
    }

    /// Inverse of [`is_on`](Self::is_on).
    #[inline]
    pub fn is_off(&mut self) -> Result<bool, P::Error> {
        self.is_on().map(|on| !on)
    }

    /// Toggle the logical state.
    ///
    /// Delegates to [`StatefulOutputPin::toggle`].  On embassy this is a single
    /// bit-band operation (`ODR ^= 1 << n`) — unconditionally flips the
    /// physical pin, which is always correct regardless of polarity.
    #[inline]
    pub fn toggle(&mut self) -> Result<(), P::Error> {
        self.pin.toggle()
    }
}

impl<P> Led<P> {
    /// Return the current polarity.
    #[inline]
    pub fn polarity(&self) -> PolarityMode {
        self.polarity
    }

    /// Consume the `Led` and return the underlying pin.
    #[inline]
    pub fn release(self) -> P {
        self.pin
    }
}

impl<P> core::fmt::Debug for Led<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Led")
            .field("polarity", &self.polarity)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "defmt")]
impl<P> defmt::Format for Led<P> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Led {{ polarity: {} }}", self.polarity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal_mock::eh1::digital::{
        Mock as PinMock, State as EState, Transaction as PinTrans,
    };

    #[test]
    fn new_active_high_sets_off() {
        let e = [PinTrans::set(EState::Low)];
        Led::new(PinMock::new(&e), PolarityMode::ActiveHigh)
            .unwrap()
            .release()
            .done();
    }

    #[test]
    fn new_active_low_sets_off() {
        let e = [PinTrans::set(EState::High)];
        Led::new(PinMock::new(&e), PolarityMode::ActiveLow)
            .unwrap()
            .release()
            .done();
    }

    #[test]
    fn from_pin_no_touch() {
        Led::from_pin(PinMock::new(&[]), PolarityMode::ActiveHigh)
            .release()
            .done();
    }

    #[test]
    fn active_high_on_off() {
        let e = [
            PinTrans::set(EState::Low),
            PinTrans::set(EState::High),
            PinTrans::set(EState::Low),
        ];
        let mut led = Led::new(PinMock::new(&e), PolarityMode::ActiveHigh).unwrap();
        led.on().unwrap();
        led.off().unwrap();
        led.release().done();
    }

    #[test]
    fn active_low_on_off() {
        let e = [
            PinTrans::set(EState::High),
            PinTrans::set(EState::Low),
            PinTrans::set(EState::High),
        ];
        let mut led = Led::new(PinMock::new(&e), PolarityMode::ActiveLow).unwrap();
        led.on().unwrap();
        led.off().unwrap();
        led.release().done();
    }

    #[test]
    fn set_state() {
        let e = [
            PinTrans::set(EState::Low),
            PinTrans::set(EState::High),
            PinTrans::set(EState::Low),
        ];
        let mut led = Led::new(PinMock::new(&e), PolarityMode::ActiveHigh).unwrap();
        led.set(true).unwrap();
        led.set(false).unwrap();
        led.release().done();
    }

    #[test]
    fn toggle() {
        let e = [PinTrans::set(EState::Low), PinTrans::toggle()];
        let mut led = Led::new(PinMock::new(&e), PolarityMode::ActiveHigh).unwrap();
        led.toggle().unwrap();
        led.release().done();
    }

    #[test]
    fn is_on_active_high() {
        let e = [PinTrans::set(EState::Low), PinTrans::get_state(EState::Low)];
        let mut led = Led::new(PinMock::new(&e), PolarityMode::ActiveHigh).unwrap();
        assert!(!led.is_on().unwrap());
        led.release().done();
    }

    #[test]
    fn is_on_active_low() {
        let e = [
            PinTrans::set(EState::High),
            PinTrans::get_state(EState::High),
        ];
        let mut led = Led::new(PinMock::new(&e), PolarityMode::ActiveLow).unwrap();
        assert!(!led.is_on().unwrap());
        led.release().done();
    }

    #[test]
    fn polarity_accessor() {
        let led = Led::from_pin(PinMock::new(&[]), PolarityMode::ActiveLow);
        assert_eq!(led.polarity(), PolarityMode::ActiveLow);
        led.release().done();
    }
}
