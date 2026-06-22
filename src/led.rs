use core::marker::PhantomData;
use embedded_hal::digital::{OutputPin, StatefulOutputPin};

use crate::polarity::{Polarity, PolarityMode};

/// Monochrome LED with compile-time polarity.
///
/// Type parameters:
/// - `P`: pin type implementing [`OutputPin`] and [`StatefulOutputPin`].
/// - `POL`: polarity marker, [`ActiveHigh`] or [`ActiveLow`] (default [`ActiveHigh`]).
///
/// No internal state cache — `is_on()` reads the hardware register (ODR) directly
/// and applies the polarity conversion.  In the embassy ecosystem `OutputPin::Error`
/// is [`Infallible`], so unwrapping results is safe.
///
/// # Examples
///
/// ```ignore
/// use status_led::{Led, ActiveLow};
///
/// let mut led = Led::<_, ActiveLow>::new(pin).unwrap();
/// led.on().unwrap();
/// led.toggle().unwrap();
/// assert!(led.is_on().unwrap());
/// ```
///
/// [`ActiveHigh`]: crate::ActiveHigh
/// [`ActiveLow`]: crate::ActiveLow
/// [`Infallible`]: core::convert::Infallible
pub struct Led<P, POL = crate::ActiveHigh> {
    pin: P,
    _polarity: PhantomData<POL>,
}

impl<P, POL> Led<P, POL> {
    /// Build from an already-configured pin without changing its level.
    ///
    /// Prefer this when the HAL already set the pin to a known state
    /// (e.g. `Output::new(pin, Level::High, Speed::Low)` for an active-low LED).
    #[inline]
    pub fn from_pin(pin: P) -> Self {
        Self {
            pin,
            _polarity: PhantomData,
        }
    }
}

impl<P: OutputPin, POL: Polarity> Led<P, POL> {
    /// Build and force the pin to the logical OFF state.
    ///
    /// Safest default — guarantees the LED starts dark regardless of the pin's
    /// reset state.
    pub fn new(mut pin: P) -> Result<Self, P::Error> {
        pin.set_state(POL::physical_off())?;
        Ok(Self {
            pin,
            _polarity: PhantomData,
        })
    }

    /// Turn the LED on (logical ON → physical level determined by polarity).
    #[inline]
    pub fn on(&mut self) -> Result<(), P::Error> {
        self.pin.set_state(POL::physical_on())
    }

    /// Turn the LED off (logical OFF → physical level determined by polarity).
    #[inline]
    pub fn off(&mut self) -> Result<(), P::Error> {
        self.pin.set_state(POL::physical_off())
    }

    /// Set the logical state: `true` = ON, `false` = OFF.
    #[inline]
    pub fn set(&mut self, state: bool) -> Result<(), P::Error> {
        if state { self.on() } else { self.off() }
    }
}

impl<P: StatefulOutputPin, POL: Polarity> Led<P, POL> {
    /// Read the logical state from the hardware register.
    ///
    /// Reads the physical pin level (ODR register on STM32), then converts
    /// through the polarity.  Requires `&mut self` because
    /// [`StatefulOutputPin::is_set_high`] does — in practice the read is a
    /// single register access with no side effects.
    #[inline]
    pub fn is_on(&mut self) -> Result<bool, P::Error> {
        self.pin.is_set_high().map(|h| POL::is_logical_on(h))
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

impl<P, POL: Polarity> Led<P, POL> {
    /// Consume the `Led` and return the underlying pin.
    #[inline]
    pub fn release(self) -> P {
        self.pin
    }

    /// Convert to a [`FlexLed`], preserving the current polarity.
    pub fn into_flex(self) -> FlexLed<P> {
        FlexLed {
            pin: self.pin,
            polarity: POL::MODE,
        }
    }
}

impl<P, POL: Polarity> core::fmt::Debug for Led<P, POL> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Led")
            .field("polarity", &core::any::type_name::<POL>())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "defmt")]
impl<P, POL: Polarity> defmt::Format for Led<P, POL> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "Led {{ polarity: {} }}",
            defmt::Display2Format(core::any::type_name::<POL>())
        )
    }
}

// ─── FlexLed ─────────────────────────────────────────

/// LED with runtime-determined polarity.
///
/// Unlike [`Led`] which bakes polarity into the type, `FlexLed` stores it as a
/// [`PolarityMode`] value.  Useful when the polarity is read from configuration
/// at runtime rather than known at compile time.  Each operation incurs one
/// extra `match` vs `Led` — negligible on Cortex-M.
///
/// # Examples
///
/// ```ignore
/// use status_led::{FlexLed, PolarityMode};
///
/// let pol = if config.active_low {
///     PolarityMode::ActiveLow
/// } else {
///     PolarityMode::ActiveHigh
/// };
/// let mut led = FlexLed::new(pin, pol).unwrap();
/// led.on().unwrap();
/// ```
pub struct FlexLed<P> {
    pin: P,
    polarity: PolarityMode,
}

impl<P> FlexLed<P> {
    /// Build from an already-configured pin without changing its level.
    #[inline]
    pub fn from_pin(pin: P, polarity: PolarityMode) -> Self {
        Self { pin, polarity }
    }
}

impl<P: OutputPin> FlexLed<P> {
    /// Build and force the pin to the logical OFF state.
    pub fn new(mut pin: P, polarity: PolarityMode) -> Result<Self, P::Error> {
        pin.set_state(polarity.physical_off())?;
        Ok(Self { pin, polarity })
    }

    /// Turn the LED on.
    #[inline]
    pub fn on(&mut self) -> Result<(), P::Error> {
        self.pin.set_state(self.polarity.physical_on())
    }

    /// Turn the LED off.
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

impl<P: StatefulOutputPin> FlexLed<P> {
    /// Read the logical state from the hardware register.
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
    #[inline]
    pub fn toggle(&mut self) -> Result<(), P::Error> {
        self.pin.toggle()
    }
}

impl<P> FlexLed<P> {
    /// Return the current polarity.
    #[inline]
    pub fn polarity(&self) -> PolarityMode {
        self.polarity
    }

    /// Consume the `FlexLed` and return the underlying pin.
    #[inline]
    pub fn release(self) -> P {
        self.pin
    }
}

impl<P, POL: Polarity> From<Led<P, POL>> for FlexLed<P> {
    fn from(led: Led<P, POL>) -> Self {
        led.into_flex()
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
        Led::<_, crate::ActiveHigh>::new(PinMock::new(&e))
            .unwrap()
            .release()
            .done();
    }

    #[test]
    fn new_active_low_sets_off() {
        let e = [PinTrans::set(EState::High)];
        Led::<_, crate::ActiveLow>::new(PinMock::new(&e))
            .unwrap()
            .release()
            .done();
    }

    #[test]
    fn from_pin_no_touch() {
        Led::<_, crate::ActiveHigh>::from_pin(PinMock::new(&[]))
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
        let mut led = Led::<_, crate::ActiveHigh>::new(PinMock::new(&e)).unwrap();
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
        let mut led = Led::<_, crate::ActiveLow>::new(PinMock::new(&e)).unwrap();
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
        let mut led = Led::<_, crate::ActiveHigh>::new(PinMock::new(&e)).unwrap();
        led.set(true).unwrap();
        led.set(false).unwrap();
        led.release().done();
    }

    #[test]
    fn toggle() {
        let e = [PinTrans::set(EState::Low), PinTrans::toggle()];
        let mut led = Led::<_, crate::ActiveHigh>::new(PinMock::new(&e)).unwrap();
        led.toggle().unwrap();
        led.release().done();
    }

    #[test]
    fn is_on_active_high() {
        let e = [PinTrans::set(EState::Low), PinTrans::get_state(EState::Low)];
        let mut led = Led::<_, crate::ActiveHigh>::new(PinMock::new(&e)).unwrap();
        assert!(!led.is_on().unwrap());
        led.release().done();
    }

    #[test]
    fn is_on_active_low() {
        let e = [
            PinTrans::set(EState::High),
            PinTrans::get_state(EState::High),
        ];
        let mut led = Led::<_, crate::ActiveLow>::new(PinMock::new(&e)).unwrap();
        assert!(!led.is_on().unwrap());
        led.release().done();
    }

    #[test]
    fn flex_new_active_low() {
        let e = [PinTrans::set(EState::High), PinTrans::set(EState::Low)];
        let mut led = FlexLed::new(PinMock::new(&e), PolarityMode::ActiveLow).unwrap();
        led.on().unwrap();
        led.release().done();
    }

    #[test]
    fn flex_from_pin_no_touch() {
        let led = FlexLed::from_pin(PinMock::new(&[]), PolarityMode::ActiveLow);
        assert_eq!(led.polarity(), PolarityMode::ActiveLow);
        led.release().done();
    }

    #[test]
    fn flex_toggle() {
        let e = [PinTrans::set(EState::Low), PinTrans::toggle()];
        let mut led = FlexLed::new(PinMock::new(&e), PolarityMode::ActiveHigh).unwrap();
        led.toggle().unwrap();
        led.release().done();
    }

    #[test]
    fn flex_is_on() {
        let e = [PinTrans::set(EState::Low), PinTrans::get_state(EState::Low)];
        let mut led = FlexLed::new(PinMock::new(&e), PolarityMode::ActiveHigh).unwrap();
        assert!(!led.is_on().unwrap());
        led.release().done();
    }

    #[test]
    fn into_flex_preserves_polarity() {
        let led = Led::<_, crate::ActiveLow>::from_pin(PinMock::new(&[]));
        let flex: FlexLed<_> = led.into_flex();
        assert_eq!(flex.polarity(), PolarityMode::ActiveLow);
        flex.release().done();
    }

    #[test]
    fn from_led_to_flex() {
        let led = Led::<_, crate::ActiveLow>::from_pin(PinMock::new(&[]));
        let flex = FlexLed::from(led);
        assert_eq!(flex.polarity(), PolarityMode::ActiveLow);
        flex.release().done();
    }
}
