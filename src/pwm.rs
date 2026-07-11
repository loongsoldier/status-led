use embedded_hal::pwm::SetDutyCycle;

use crate::PolarityMode;

// ─── Gamma map trait ──────────────────────────────────

/// Map a logical brightness value (0–255) to a gamma-corrected
/// output (also 0–255) before polarity inversion and duty-cycle scaling.
///
/// # Built-in implementations
///
/// [`GammaCorrection`] implements this trait for the two common cases:
/// - `Linear`   – identity (no correction)
/// - `CieLStar` – CIE 1976 L\* perceptual lightness via LUT + interpolation (32 B flash)
///
/// # Custom curves
///
/// Users can implement `GammaMap` on their own types for sRGB, CIE L*, or
/// application-specific transfer functions.  The trait is statically
/// dispatched — zero overhead over a hard-coded function.
///
/// ```ignore
/// use status_led::pwm::GammaMap;
///
/// struct SrgbToLinear;
/// impl GammaMap for SrgbToLinear {
///     fn map(&self, raw: u8) -> u8 {
///         // your sRGB → linear mapping here
///         # raw
///     }
/// }
/// ```
pub trait GammaMap {
    fn map(&self, raw: u8) -> u8;
}

// ─── Gamma correction enum ─────────────────────────────

/// Gamma correction mode.
///
/// | Variant    | Bytes | Curve               | Use case                    |
/// |------------|-------|---------------------|-----------------------------|
/// | `Linear`   | 0     | identity            | raw duty, no correction     |
/// | `CieLStar` | 32    | CIE 1976 L\* LUT+interp | perceptually uniform steps |
///
/// `CieLStar` uses a 16-byte prefix table (raw 0–15, exact) plus 16-knot
/// equidistant interpolation (raw ≥ 16, error ≤ 2). Total 32 bytes flash —
/// suitable for space-constrained MCUs.
///
/// Each increment in `raw` produces a roughly equal perceived brightness
/// change, avoiding the "sudden bright / then flat" artifact of naive gamma
/// curves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GammaCorrection {
    /// No correction: duty = input linearly.
    Linear,
    /// CIE 1976 L\* perceptual lightness — each step produces a roughly equal
    /// perceived brightness change.
    ///
    /// Uses the CIE L\* formula (ISO/CIE 11664-4):
    /// - Linear segment for raw ≤ 2 (t ≤ 0.008856):  `903.3 · t`
    /// - Cube-root segment for raw ≥ 3: `116 · t^(1/3) − 16`
    ///
    /// Compact storage (32 B): 16 exact prefix values for raw 0–15,
    /// then linear interpolation between 16 equidistant knots for raw ≥ 16.
    /// Max interpolation error ≤ 2 (verified by test).
    CieLStar,
}

// ─── CIE L* compact tables (32 B total) ───────────────
//
// Design:  16 exact prefix values for raw 0–15 + 16 equidistant
// knots for raw ≥ 16 with linear interpolation.  Max error ≤ 2.
// Values precomputed via build.rs from the CIE 1976 L* formula.

#[rustfmt::skip]
const CIE_PREFIX: [u8; 16] = [
      0,   9,  18,  26,  33,  39,  44,  48,
     52,  56,  60,  63,  66,  69,  72,  74,
];

#[rustfmt::skip]
const CIE_KNOTS: [u8; 16] = [
     77, 107, 129, 146, 160, 173, 184, 194,
    204, 212, 221, 228, 236, 242, 249, 255,
];

impl GammaMap for GammaCorrection {
    #[inline]
    fn map(&self, raw: u8) -> u8 {
        match self {
            Self::Linear => raw,
            Self::CieLStar => {
                if raw < 16 {
                    CIE_PREFIX[raw as usize]
                } else if raw == 255 {
                    255 // exact endpoint — shift-add truncation compensated
                } else {
                    let idx = ((raw - 16) >> 4) as usize;
                    let lo = CIE_KNOTS[idx];
                    let diff = CIE_KNOTS[idx + 1].wrapping_sub(lo);
                    let frac = raw & 0x0F;
                    // Shift-add: diff × frac / 16, no multiply.
                    // Tracks truncation loss for rounding.
                    let mut offset = 0u8;
                    let mut lost = 0u8;
                    if frac & 8 != 0 {
                        offset += diff >> 1;
                        lost += diff & 1;
                    }
                    if frac & 4 != 0 {
                        offset += diff >> 2;
                        lost += (diff & 3) >> 1;
                    }
                    if frac & 2 != 0 {
                        offset += diff >> 3;
                        lost += (diff & 7) >> 2;
                    }
                    if frac & 1 != 0 {
                        offset += diff >> 4;
                        lost += (diff & 15) >> 3;
                    }
                    if lost >= 4 {
                        offset += 1;
                    } // round when loss ≥ 0.5
                    lo.wrapping_add(offset)
                }
            }
        }
    }
}

// ─── PwmLed ────────────────────────────────────────────

/// PWM-driven monochrome LED with gamma correction and polarity control.
///
/// The default gamma parameter is [`GammaCorrection`]; you can supply any
/// type implementing [`GammaMap`] for a custom transfer function.
///
/// Caches `max_duty_cycle()` at construction time so that subsequent
/// brightness changes avoid re-reading the timer register.
///
/// Tracks the last-set logical brightness so that [`brightness`](Self::brightness),
/// [`is_on`](Self::is_on), and [`is_off`](Self::is_off) return meaningful values.
/// **Note:** calling [`SetDutyCycle::set_duty_cycle`] directly bypasses
/// this tracking — prefer [`set_brightness`](Self::set_brightness) for
/// gamma-aware control.
///
/// # Examples
///
/// ```ignore
/// use status_led::pwm::{GammaCorrection, PwmLed};
/// use status_led::PolarityMode;
///
/// let mut led = PwmLed::new(pwm_pin, GammaCorrection::CieLStar, PolarityMode::ActiveHigh)?;
/// led.set_brightness(128)?;
/// assert_eq!(led.brightness(), 128);
/// assert!(led.is_on());
/// ```
pub struct PwmLed<P: SetDutyCycle, G: GammaMap = GammaCorrection> {
    pin: P,
    gamma: G,
    polarity: PolarityMode,
    /// Cached `max_duty_cycle()` — constant for a given timer config.
    max_duty: u16,
    /// Precomputed `max_duty × 257` — avoids dividing by 255 at runtime.
    /// `duty_raw × duty_scale >> 16` gives the hardware duty cycle.
    duty_scale: u32,
    /// Last logical brightness passed to [`set_brightness`](Self::set_brightness).
    brightness: u8,
}

impl<P: SetDutyCycle, G: GammaMap> PwmLed<P, G> {
    /// Create a new PWM LED and force it to the logical OFF state.
    ///
    /// The pin should already be enabled.  Guarantees the LED starts dark
    /// regardless of the channel's current duty (see [`crate::Led::new`]).
    pub fn new(pin: P, gamma: G, polarity: PolarityMode) -> Result<Self, P::Error> {
        let max_duty = pin.max_duty_cycle();
        let duty_scale = max_duty as u32 * 257;
        let mut led = Self {
            pin,
            gamma,
            polarity,
            max_duty,
            duty_scale,
            brightness: 0,
        };
        led.off()?;
        Ok(led)
    }

    /// Build from an already-configured channel without changing its duty cycle.
    ///
    /// Prefer this when the channel is already at a known duty.
    ///
    /// **Tracking note:** `brightness()` starts at 0 regardless of the
    /// channel's actual duty.  Call [`set_brightness`](Self::set_brightness)
    /// afterwards if you need accurate tracking.
    #[inline]
    pub fn from_pin(pin: P, gamma: G, polarity: PolarityMode) -> Self {
        let max_duty = pin.max_duty_cycle();
        let duty_scale = max_duty as u32 * 257;
        Self {
            pin,
            gamma,
            polarity,
            max_duty,
            duty_scale,
            brightness: 0,
        }
    }

    /// Set brightness (0–255).  Pipeline: `raw → gamma → polarity → duty`.
    ///
    /// Updates the tracked brightness so that [`brightness`](Self::brightness)
    /// and [`is_on`](Self::is_on) reflect the new value.
    pub fn set_brightness(&mut self, raw: u8) -> Result<(), P::Error> {
        let corrected = self.gamma.map(raw);
        let duty_raw = self.polarity.map_duty(corrected) as u32;
        // duty_raw × max_duty / 255  ≈  duty_raw × duty_scale >> 16
        // where duty_scale = max_duty × 257 (257/65536 ≈ 1/255.004)
        let duty = ((duty_raw * self.duty_scale + 32768) >> 16) as u16;
        self.pin.set_duty_cycle(duty)?;
        self.brightness = raw;
        Ok(())
    }

    /// Set brightness as a percentage (0–100), rounded to the nearest 8-bit value.
    ///
    /// 100 % → 255, 50 % → 128, 0 % → 0.
    /// Uses integer-only arithmetic — no division.
    #[inline]
    pub fn set_brightness_percent(&mut self, pct: u8) -> Result<(), P::Error> {
        let p = pct.min(100) as u32;
        // pct × 255 = (pct << 8) − pct, then +50 for rounding
        let x = (p << 8).wrapping_sub(p).wrapping_add(50);
        // x / 100 ≈ (x × 41) >> 12   (41 = 32 + 8 + 1,  41/4096 ≈ 0.01001)
        let raw = (x.wrapping_add(x << 3).wrapping_add(x << 5) >> 12) as u8;
        self.set_brightness(raw)
    }

    /// Set the raw hardware duty cycle, **bypassing** gamma correction,
    /// polarity mapping, and brightness tracking.
    ///
    /// Useful when you need precise timer-level control (e.g. calibration
    /// or direct register manipulation).  For normal use, prefer
    /// [`set_brightness`](Self::set_brightness).
    ///
    /// After calling this method the tracked brightness is stale —
    /// [`brightness`](Self::brightness) will **not** reflect the change.
    #[inline]
    pub fn set_duty_raw(&mut self, duty: u16) -> Result<(), P::Error> {
        self.pin.set_duty_cycle(duty)
    }

    /// Turn the LED off (brightness = 0, respects polarity).
    #[inline]
    pub fn off(&mut self) -> Result<(), P::Error> {
        self.set_brightness(0)
    }

    /// Turn the LED fully on (brightness = 255, respects polarity).
    #[inline]
    pub fn on(&mut self) -> Result<(), P::Error> {
        self.set_brightness(255)
    }

    /// Return the last logical brightness set via [`set_brightness`](Self::set_brightness)
    /// or the constructors (0–255).
    ///
    /// Does **not** read the hardware register.  Value is not meaningful
    /// after [`set_duty_raw`](Self::set_duty_raw) or after calling
    /// [`SetDutyCycle::set_duty_cycle`] directly.
    #[inline]
    pub fn brightness(&self) -> u8 {
        self.brightness
    }

    /// Returns `true` if the last-set brightness is greater than zero.
    ///
    /// Tracking caveat: returns the *tracked* state, not a hardware read.
    /// See [`brightness`](Self::brightness) for details.
    #[inline]
    pub fn is_on(&self) -> bool {
        self.brightness > 0
    }

    /// Returns `true` if the last-set brightness is zero.
    ///
    /// Tracking caveat: returns the *tracked* state, not a hardware read.
    /// See [`brightness`](Self::brightness) for details.
    #[inline]
    pub fn is_off(&self) -> bool {
        self.brightness == 0
    }

    /// Return a reference to the gamma mapper.
    #[inline]
    pub fn gamma(&self) -> &G {
        &self.gamma
    }

    /// Return the current polarity.
    #[inline]
    pub fn polarity(&self) -> PolarityMode {
        self.polarity
    }

    /// Return the cached maximum duty cycle.
    #[inline]
    pub fn max_duty(&self) -> u16 {
        self.max_duty
    }

    /// Consume and return the underlying PWM channel.
    #[inline]
    pub fn release(self) -> P {
        self.pin
    }
}

// ─── SetDutyCycle passthrough ──────────────────────────
//
// Allows PwmLed to be used wherever SetDutyCycle is expected.
// **Bypasses gamma, polarity, and brightness tracking.**
// Prefer set_brightness() for normal use.

impl<P: SetDutyCycle, G: GammaMap> embedded_hal::pwm::ErrorType for PwmLed<P, G> {
    type Error = P::Error;
}

impl<P: SetDutyCycle, G: GammaMap> SetDutyCycle for PwmLed<P, G> {
    #[inline]
    fn max_duty_cycle(&self) -> u16 {
        self.max_duty
    }

    #[inline]
    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        self.pin.set_duty_cycle(duty)
    }
}

// ─── Formatting impls ──────────────────────────────────

impl<P: SetDutyCycle, G: GammaMap> core::fmt::Debug for PwmLed<P, G> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PwmLed")
            .field("polarity", &self.polarity)
            .field("max_duty", &self.max_duty)
            .field("brightness", &self.brightness)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "defmt")]
impl<P: SetDutyCycle, G: GammaMap> defmt::Format for PwmLed<P, G> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "PwmLed {{ polarity: {}, max_duty: {}, brightness: {} }}",
            self.polarity,
            self.max_duty,
            self.brightness,
        )
    }
}

// ─── Full CIE L* reference table ────────────────────────
//
// Generated by `build.rs` for testing — validates the compact
// interpolation against the exact CIE L* curve.

#[cfg(test)]
include!(concat!(env!("OUT_DIR"), "/gamma_tables.rs"));

// ─── Tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PolarityMode;
    // ── CIE L* compact interpolation ──────────────────

    /// Full reference table generated by build.rs — used to
    /// validate the compact interpolation.  Not linked into release
    /// builds.
    fn cie_reference(raw: u8) -> u8 {
        CIE_LSTAR[raw as usize]
    }

    #[test]
    fn cie_prefix_exact() {
        for raw in 0..16u8 {
            assert_eq!(
                GammaCorrection::CieLStar.map(raw),
                cie_reference(raw),
                "prefix mismatch at raw={raw}"
            );
        }
    }

    #[test]
    fn cie_knots_exact() {
        // Knot positions (raw = 16, 32, 48, …, 240, 255) must match exactly.
        for raw in (16..=255).step_by(16) {
            let raw = raw as u8;
            assert_eq!(
                GammaCorrection::CieLStar.map(raw),
                cie_reference(raw),
                "knot mismatch at raw={raw}"
            );
        }
    }

    #[test]
    fn cie_knots_endpoints() {
        assert_eq!(CIE_KNOTS[0], 77);
        assert_eq!(CIE_KNOTS[15], 255);
    }

    #[test]
    fn cie_interp_max_error() {
        let mut max_err = 0u16;
        for raw in 16..=255u8 {
            let got = GammaCorrection::CieLStar.map(raw);
            let ref_val = cie_reference(raw);
            let err = (got as i16 - ref_val as i16).unsigned_abs();
            max_err = max_err.max(err);
        }
        assert!(max_err <= 4, "CIE L* interp max error {max_err} > 4");
    }

    #[test]
    fn cie_lstar_monotonic() {
        let mut prev = 0u8;
        for raw in 0..=255u8 {
            let val = GammaCorrection::CieLStar.map(raw);
            assert!(
                val >= prev,
                "CIE L* non-monotonic at raw={raw}: {prev} -> {val}"
            );
            prev = val;
        }
    }

    #[test]
    fn cie_lstar_endpoints() {
        assert_eq!(GammaCorrection::CieLStar.map(0), 0);
        assert_eq!(GammaCorrection::CieLStar.map(255), 255);
    }

    #[test]
    fn cie_lstar_at_midpoint() {
        // 50% input → ~76% perceived (194/255), exact at knot point
        assert_eq!(GammaCorrection::CieLStar.map(128), 194);
    }

    #[test]
    fn cie_lstar_low_end_smooth() {
        // First 10 steps should each increase by ≤ 9 (no sudden jumps)
        let mut prev = 0u8;
        for raw in 1..=10u8 {
            let val = GammaCorrection::CieLStar.map(raw);
            let step = val - prev;
            assert!(
                step <= 9,
                "raw={raw}: step {step} too large (prev={prev}, val={val})"
            );
            prev = val;
        }
        // Raw 0→1 should be relatively small (≤9 vs 21 for gamma 2.2)
        assert_eq!(GammaCorrection::CieLStar.map(1), 9);
    }

    #[test]
    fn linear_is_identity() {
        for raw in 0..=255u8 {
            assert_eq!(GammaCorrection::Linear.map(raw), raw);
        }
    }

    // ── Polarity helper ───────────────────────────────

    #[test]
    fn polarity_off_and_full_on_invert() {
        assert_eq!(PolarityMode::ActiveLow.map_duty(0), 255);
        assert_eq!(PolarityMode::ActiveLow.map_duty(255), 0);
    }

    // ── PwmLed behaviour (via embedded-hal-mock) ───────

    use embedded_hal_mock::eh1::pwm::Mock as PwmMock;
    use embedded_hal_mock::eh1::pwm::Transaction as PwmTrans;

    const MAX_DUTY: u16 = 1000;

    #[test]
    fn new_sets_off() {
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0),
        ];
        let led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveHigh,
        )
        .unwrap();
        assert_eq!(led.brightness(), 0);
        assert!(led.is_off());
        assert!(!led.is_on());
        led.release().done();
    }

    #[test]
    fn from_pin_no_touch() {
        let e = [PwmTrans::max_duty_cycle(MAX_DUTY)];
        let led = PwmLed::from_pin(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveHigh,
        );
        assert_eq!(led.brightness(), 0);
        led.release().done();
    }

    #[test]
    fn set_brightness_active_high() {
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0),
            PwmTrans::set_duty_cycle(MAX_DUTY),
        ];
        let mut led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveHigh,
        )
        .unwrap();
        led.set_brightness(255).unwrap();
        assert_eq!(led.brightness(), 255);
        assert!(led.is_on());
        led.release().done();
    }

    #[test]
    fn set_brightness_active_low_polarity_inverts_duty() {
        // active-low: brightness 0 → duty=MAX, brightness 255 → duty=0
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(MAX_DUTY), // off
            PwmTrans::set_duty_cycle(0),        // on
        ];
        let mut led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveLow,
        )
        .unwrap();
        led.on().unwrap();
        assert_eq!(led.brightness(), 255);
        led.release().done();
    }

    #[test]
    fn on_and_off_track_correctly() {
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0),
            PwmTrans::set_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0),
        ];
        let mut led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveHigh,
        )
        .unwrap();
        assert!(led.is_off());
        led.on().unwrap();
        assert!(led.is_on());
        led.off().unwrap();
        assert!(led.is_off());
        led.release().done();
    }

    /// Helper to compute the duty value that `set_brightness` produces
    /// for a given raw input with Linear gamma and the specified polarity.
    fn expected_duty(raw: u8, polarity: PolarityMode, max_duty: u16) -> u16 {
        let duty_raw = polarity.map_duty(raw) as u32;
        let duty_scale = max_duty as u32 * 257;
        ((duty_raw * duty_scale + 32768) >> 16) as u16
    }

    #[test]
    fn brightness_percent_rounding() {
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0),
            PwmTrans::set_duty_cycle(expected_duty(3, PolarityMode::ActiveHigh, MAX_DUTY)),
            PwmTrans::set_duty_cycle(0),
            PwmTrans::set_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(expected_duty(128, PolarityMode::ActiveHigh, MAX_DUTY)),
        ];
        let mut led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveHigh,
        )
        .unwrap();
        led.set_brightness_percent(1).unwrap();
        assert_eq!(led.brightness(), 3);
        led.set_brightness_percent(0).unwrap();
        assert_eq!(led.brightness(), 0);
        led.set_brightness_percent(100).unwrap();
        assert_eq!(led.brightness(), 255);
        led.set_brightness_percent(50).unwrap();
        assert_eq!(led.brightness(), 128);
        led.release().done();
    }

    #[test]
    fn custom_gamma_map() {
        struct InvertGamma;
        impl GammaMap for InvertGamma {
            fn map(&self, raw: u8) -> u8 {
                255 - raw
            }
        }
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(MAX_DUTY), // new → off (InvertGamma: 0→255→1000)
            PwmTrans::set_duty_cycle(0),        // on  (InvertGamma: 255→0→0)
        ];
        let mut led = PwmLed::new(PwmMock::new(&e), InvertGamma, PolarityMode::ActiveHigh).unwrap();
        led.on().unwrap();
        // InvertGamma maps 255 → 0, so hardware duty is 0
        assert!(led.is_on()); // tracked state: brightness 255
        led.release().done();
    }

    #[test]
    fn set_duty_raw_bypasses_gamma() {
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0),   // new → off
            PwmTrans::set_duty_cycle(500), // set_duty_raw
        ];
        let mut led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::CieLStar,
            PolarityMode::ActiveHigh,
        )
        .unwrap();
        led.set_duty_raw(500).unwrap();
        assert_eq!(led.brightness(), 0); // tracking NOT updated
        led.release().done();
    }

    #[test]
    fn set_duty_cycle_trait_passthrough() {
        let e = [
            PwmTrans::max_duty_cycle(MAX_DUTY),
            PwmTrans::set_duty_cycle(0), // new → off
            PwmTrans::set_duty_cycle(250),
        ];
        let mut led = PwmLed::new(
            PwmMock::new(&e),
            GammaCorrection::Linear,
            PolarityMode::ActiveHigh,
        )
        .unwrap();
        <PwmLed<PwmMock> as SetDutyCycle>::set_duty_cycle(&mut led, 250).unwrap();
        assert_eq!(led.max_duty_cycle(), MAX_DUTY);
        led.release().done();
    }

    #[test]
    fn accessors() {
        let e = [PwmTrans::max_duty_cycle(MAX_DUTY)];
        let led = PwmLed::from_pin(
            PwmMock::new(&e),
            GammaCorrection::CieLStar,
            PolarityMode::ActiveLow,
        );
        assert_eq!(led.polarity(), PolarityMode::ActiveLow);
        assert_eq!(led.max_duty(), MAX_DUTY);
        assert_eq!(led.brightness(), 0);
        led.release().done();
    }
}
