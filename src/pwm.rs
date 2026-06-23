use core::marker::PhantomData;
use embedded_hal::pwm::SetDutyCycle;

use crate::polarity::{Polarity, PolarityMode};

/// Gamma correction mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GammaCorrection {
    /// No correction: duty = input linearly.
    Linear,
    /// Power-law gamma=2.2 correction so `set_brightness(128)` appears ~50% bright.
    ///
    /// Uses a 16-byte prefix table (raw 0-15, exact) + 16-knot equidistant
    /// interpolation (raw >= 16, error <= 2).  Total 32 bytes flash, zero
    /// error in the critical dim range where the eye is most sensitive.
    SRGB,
}

impl GammaCorrection {
    /// Map a raw brightness value (0-255) to gamma-corrected output (0-255).
    #[inline]
    pub fn map(&self, raw: u8) -> u8 {
        match self {
            Self::Linear => raw,
            Self::SRGB => {
                if raw < 16 {
                    GAMMA_PREFIX[raw as usize]
                } else {
                    let idx = ((raw - 16) >> 4) as usize;
                    let lo = GAMMA_KNOTS_HI[idx];
                    let hi = GAMMA_KNOTS_HI[idx + 1];
                    let frac = raw & 0x0F;
                    let offset = ((hi.wrapping_sub(lo) as u16 * frac as u16 + 8) / 16) as u8;
                    lo.wrapping_add(offset)
                }
            }
        }
    }
}

/// PWM-driven LED with brightness control and polarity.
///
/// Queries `max_duty_cycle()` from the pin on each `set_brightness()` call —
/// no need to store it separately.
pub struct PwmLed<P: SetDutyCycle, POL = crate::ActiveHigh> {
    pin: P,
    gamma: GammaCorrection,
    _polarity: PhantomData<POL>,
}

impl<P: SetDutyCycle, POL: Polarity> PwmLed<P, POL> {
    /// Create a new PWM LED.  The pin should already be enabled.
    pub fn new(pin: P, gamma: GammaCorrection) -> Self {
        Self {
            pin,
            gamma,
            _polarity: PhantomData,
        }
    }

    /// Set brightness (0-255).  Pipeline: `raw -> gamma -> polarity -> duty`.
    pub fn set_brightness(&mut self, raw: u8) -> Result<(), P::Error> {
        let corrected = self.gamma.map(raw);
        let duty_raw = POL::map_duty(corrected);
        let duty = (duty_raw as u32 * self.pin.max_duty_cycle() as u32 / 255) as u16;
        self.pin.set_duty_cycle(duty)
    }

    pub fn set_brightness_percent(&mut self, pct: u8) -> Result<(), P::Error> {
        let raw = ((pct.min(100) as u16 * 255) / 100) as u8;
        self.set_brightness(raw)
    }

    /// Turn the LED off (respects polarity).
    #[inline]
    pub fn off(&mut self) -> Result<(), P::Error> {
        self.set_brightness(0)
    }

    /// Turn the LED fully on (respects polarity).
    #[inline]
    pub fn full_on(&mut self) -> Result<(), P::Error> {
        self.set_brightness(255)
    }

    /// Consume and return the underlying PWM channel.
    #[inline]
    pub fn release(self) -> P {
        self.pin
    }
}

// ─── FlexPwmLed ───────────────────────────────────────

/// PWM LED with runtime-determined polarity.
///
/// Like [`PwmLed`] but polarity is a [`PolarityMode`] value instead of a
/// compile-time type parameter.
pub struct FlexPwmLed<P: SetDutyCycle> {
    pin: P,
    gamma: GammaCorrection,
    polarity: PolarityMode,
}

impl<P: SetDutyCycle> FlexPwmLed<P> {
    pub fn new(pin: P, gamma: GammaCorrection, polarity: PolarityMode) -> Self {
        Self {
            pin,
            gamma,
            polarity,
        }
    }

    pub fn set_brightness(&mut self, raw: u8) -> Result<(), P::Error> {
        let corrected = self.gamma.map(raw);
        let duty_raw = self.polarity.map_duty(corrected);
        let duty = (duty_raw as u32 * self.pin.max_duty_cycle() as u32 / 255) as u16;
        self.pin.set_duty_cycle(duty)
    }

    pub fn set_brightness_percent(&mut self, pct: u8) -> Result<(), P::Error> {
        let raw = ((pct.min(100) as u16 * 255) / 100) as u8;
        self.set_brightness(raw)
    }

    #[inline]
    pub fn off(&mut self) -> Result<(), P::Error> {
        self.set_brightness(0)
    }
    #[inline]
    pub fn full_on(&mut self) -> Result<(), P::Error> {
        self.set_brightness(255)
    }
    #[inline]
    pub fn polarity(&self) -> PolarityMode {
        self.polarity
    }
    #[inline]
    pub fn release(self) -> P {
        self.pin
    }
}

impl<P: SetDutyCycle, POL: Polarity> From<PwmLed<P, POL>> for FlexPwmLed<P> {
    fn from(led: PwmLed<P, POL>) -> Self {
        Self {
            pin: led.pin,
            gamma: led.gamma,
            polarity: POL::MODE,
        }
    }
}

// ─── Gamma 2.2 tables (32 B total) ──────────────────────

#[rustfmt::skip]
const GAMMA_PREFIX: [u8; 16] = [
    0, 21, 28, 34, 39, 43, 46, 50, 53, 56, 59, 61, 64, 66, 68, 70,
];

#[rustfmt::skip]
const GAMMA_KNOTS_HI: [u8; 16] = [
   72,  99, 119, 136, 151, 164, 175, 186,
  197, 206, 215, 224, 232, 240, 248, 255,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActiveLow;

    fn reference_gamma(raw: u8) -> u8 {
        #[rustfmt::skip]
        const FULL: [u8; 256] = [
            0,  21,  28,  34,  39,  43,  46,  50,  53,  56,  59,  61,  64,  66,  68,  70,
           72,  74,  76,  78,  80,  82,  84,  85,  87,  89,  90,  92,  93,  95,  96,  98,
           99, 101, 102, 103, 105, 106, 107, 109, 110, 111, 112, 114, 115, 116, 117, 118,
          119, 120, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
          136, 137, 138, 139, 140, 141, 142, 143, 144, 144, 145, 146, 147, 148, 149, 150,
          151, 151, 152, 153, 154, 155, 156, 156, 157, 158, 159, 160, 160, 161, 162, 163,
          164, 164, 165, 166, 167, 167, 168, 169, 170, 170, 171, 172, 173, 173, 174, 175,
          175, 176, 177, 178, 178, 179, 180, 180, 181, 182, 182, 183, 184, 184, 185, 186,
          186, 187, 188, 188, 189, 190, 190, 191, 192, 192, 193, 194, 194, 195, 195, 196,
          197, 197, 198, 199, 199, 200, 200, 201, 202, 202, 203, 203, 204, 205, 205, 206,
          206, 207, 207, 208, 209, 209, 210, 210, 211, 212, 212, 213, 213, 214, 214, 215,
          215, 216, 217, 217, 218, 218, 219, 219, 220, 220, 221, 221, 222, 223, 223, 224,
          224, 225, 225, 226, 226, 227, 227, 228, 228, 229, 229, 230, 230, 231, 231, 232,
          232, 233, 233, 234, 234, 235, 235, 236, 236, 237, 237, 238, 238, 239, 239, 240,
          240, 241, 241, 242, 242, 243, 243, 244, 244, 245, 245, 246, 246, 247, 247, 248,
          248, 249, 249, 249, 250, 250, 251, 251, 252, 252, 253, 253, 254, 254, 255, 255,
        ];
        FULL[raw as usize]
    }

    #[test]
    fn prefix_exact() {
        for raw in 0..16u8 {
            assert_eq!(GammaCorrection::SRGB.map(raw), reference_gamma(raw));
        }
    }

    #[test]
    fn knots_endpoints() {
        assert_eq!(GAMMA_KNOTS_HI[0], 72);
        assert_eq!(GAMMA_KNOTS_HI[15], 255);
    }

    #[test]
    fn interp_exact_at_knots() {
        for raw in (16..=255).step_by(16) {
            assert_eq!(
                GammaCorrection::SRGB.map(raw as u8),
                reference_gamma(raw as u8)
            );
        }
    }

    #[test]
    fn interp_max_error() {
        let gamma = GammaCorrection::SRGB;
        let mut max_err = 0u16;
        for raw in 16..=255u8 {
            let got = gamma.map(raw);
            let err = (got as i16 - reference_gamma(raw) as i16).unsigned_abs();
            max_err = max_err.max(err);
        }
        assert!(max_err <= 2, "max error {max_err}");
    }

    #[test]
    fn gamma_midpoint() {
        assert_eq!(GammaCorrection::SRGB.map(128), 186);
    }

    #[test]
    fn polarity_off_and_full_on_invert() {
        assert_eq!(ActiveLow::map_duty(0), 255);
        assert_eq!(ActiveLow::map_duty(255), 0);
    }
}
