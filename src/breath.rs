//! Sinusoidal breathing effect for monochrome LEDs.
//!
//! [`Breath`] generates a stream of brightness values following a
//! sine wave, suitable for driving a PWM LED with a natural "breathing"
//! fade-in / fade-out.
//!
//! The brightness type is `u8` (0–255) by default. Enable the
//! `brightness-12bit` feature to use `arbitrary_int::u12` (0–4095).
//!
//! Enable the `breath` feature in `Cargo.toml`:
//!
//! ```toml
//! status-led = { version = "0.7", features = ["breath"] }
//! ```
//!
//! Combine with `pwm` for [`BreathLed`], a ready-to-use PWM LED wrapper:
//!
//! ```toml
//! status-led = { version = "0.7", features = ["breath", "pwm"] }
//! ```

use crate::brightness::{self, Brightness};

// ── CORDIC sine ─────────────────────────────────────────
//
// Phase: u16, full circle = 65536, π/2 = 16384.
// atan(2^(-i)) in Q14 (π/2 = 16384).
// 15 iterations, shift-add only — zero mul, zero float.
//
// X_INIT = round(32768 × 256 × K_15); K_15 ≈ 0.607252935.
// → y_N = X_INIT/K × sinθ ≈ 8,388,608 × sinθ
// → (y >> 16) + 128 ∈ [−128+128, 128+128] = [0, 256] → clamp → u8

/// atan(2^(-i)) × 16384/(π/2), i = 0..14
const CORDIC_ATAN: [i16; 15] = [
    8192, 4836, 2555, 1297, 651, 326, 163, 81, 41, 20, 10, 5, 3, 1, 1,
];

/// X_INIT = round(32768 × 256 × K) = 5,094,520
const X_INIT: i32 = 5_094_520;

/// Core CORDIC sine — returns a raw 8-bit value in `0..=255`.
///
/// Phase is a `u16` where 0 → 0°, 16384 → 90°, 32768 → 180°.
/// Uses CORDIC rotation mode with shift-add only — zero multiplication,
/// zero lookup tables (beyond the 30-byte atan table), zero floating
/// point.
#[inline]
fn cordic_sin_raw(phase: u16) -> u8 {
    // ── Quadrant reduction: map to [0, π/2] = [0, 16384] ──
    let mut negate = false;
    let mut angle: i32 = phase as i32;

    if angle >= 32768 {
        // Q3 / Q4: sin is negative
        negate = true;
        angle -= 32768;
    }
    if angle > 16384 {
        // Q2: sin(θ) = sin(π − θ)
        angle = 32768 - angle;
    }
    // angle ∈ [0, 16384], maps linearly to [0, π/2]

    // ── CORDIC rotation (15 iterations, shift-add) ──────
    let mut x: i32 = X_INIT;
    let mut y: i32 = 0;

    for (i, &atan) in CORDIC_ATAN.iter().enumerate() {
        let d: i32 = if angle >= 0 { 1 } else { -1 };
        let x_next = x - d * (y >> i);
        let y_next = y + d * (x >> i);
        angle -= d * atan as i32;
        x = x_next;
        y = y_next;
    }

    if negate {
        y = -y;
    }

    // y ∈ [-8_388_608, 8_388_608], >>16 → [-128, 128], +128 → [0, 256], clamp
    ((y >> 16) + 128).clamp(0, 255) as u8
}

/// Compute sin(phase) mapped to the current [`Brightness`] range (0..=MAX).
///
/// Calls the 8-bit CORDIC core, then scales the result to the target bit depth.
#[inline]
fn cordic_sin(phase: u16) -> Brightness {
    let raw8 = cordic_sin_raw(phase);
    brightness::from_u32_clamped(raw8 as u32 * brightness::MAX_BRIGHTNESS / 255)
}

// ── Breath ────────────────────────────────────────────

/// Breathing effect generator.
///
/// Produces a sequence of brightness values following a sinusoidal
/// pattern, suitable for driving an LED with a natural "breathing" effect
/// (smooth fade-in / fade-out).
///
/// Uses the CORDIC algorithm for sine computation — zero floating point,
/// zero hardware multiply, zero large lookup tables.
///
/// # Example
///
/// ```ignore
/// use status_led::breath::Breath;
///
/// // 12.8 second cycle, updated every 50 ms
/// let mut breath = Breath::new(Duration::from_millis(12_800), Duration::from_millis(50));
///
/// loop {
///     led.set_brightness(breath.next()).unwrap();
///     Timer::after_millis(50).await;
/// }
/// ```
pub struct Breath {
    phase: u16,
    phase_step: u16,
    interval: embassy_time::Duration,
}

impl Breath {
    /// Create a new breathing generator.
    ///
    /// * `cycle` — duration of one full breath cycle (bright → dark →
    ///   bright) as a [`Duration`](embassy_time::Duration).
    /// * `interval` — time between [`next`](Self::next) calls as a
    ///   [`Duration`](embassy_time::Duration).  Should be ≤ `cycle`.
    ///
    /// # Panics
    ///
    /// Panics if `cycle` or `interval` is zero milliseconds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 8-second cycle at 20 Hz → 400 steps
    /// let breath = Breath::new(Duration::from_millis(8000), Duration::from_millis(50));
    /// ```
    #[track_caller]
    pub fn new(cycle: embassy_time::Duration, interval: embassy_time::Duration) -> Self {
        let cycle_ms = cycle.as_millis();
        let interval_ms = interval.as_millis();
        assert!(cycle_ms > 0, "cycle duration must be > 0 ms");
        assert!(interval_ms > 0, "interval duration must be > 0 ms");
        // phase_step = 65536 × interval_ms / cycle_ms
        let step = ((65536u64 * interval_ms) / cycle_ms) as u16;
        Self {
            phase: 0,
            phase_step: step.max(1),
            interval,
        }
    }

    /// Advance the phase and return the next brightness value.
    ///
    /// The brightness follows a sinusoidal curve: starts near 0, rises to
    /// max at the peak, then falls back to near 0.
    #[inline]
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Brightness {
        let brightness = cordic_sin(self.phase);
        self.phase = self.phase.wrapping_add(self.phase_step);
        brightness
    }

    /// Reset the phase to the start of the cycle (brightness minimum).
    #[inline]
    pub fn reset(&mut self) {
        self.phase = 0;
    }
}

// ── Debug / defmt impls ───────────────────────────────

impl core::fmt::Debug for Breath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Breath")
            .field("phase", &self.phase)
            .field("phase_step", &self.phase_step)
            .field("interval_ms", &self.interval.as_millis())
            .finish()
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Breath {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "Breath {{ phase: {}, step: {}, interval_ms: {} }}",
            self.phase,
            self.phase_step,
            self.interval.as_millis()
        )
    }
}

// ── BreathLed (requires pwm) ──────────────────────────

use crate::PolarityMode;
use crate::pwm::{GammaCorrection, GammaMap, PwmLed};
use embedded_hal::pwm::SetDutyCycle;

/// PWM LED with a built-in sinusoidal breathing effect.
///
/// Wraps a [`PwmLed`] together with a [`Breath`] generator, providing a
/// single [`breathe`](Self::breathe) call that advances the animation and
/// updates the LED brightness.
///
/// Requires the `breath` feature (which enables `pwm`):
///
/// ```toml
/// status-led = { version = "0.7", features = ["breath"] }
/// ```
///
/// # Example
///
/// ```ignore
/// use status_led::breath::BreathLed;
/// use status_led::pwm::GammaCorrection;
/// use status_led::PolarityMode;
///
/// let mut led = BreathLed::new(
///     pwm_channel,
///     GammaCorrection::CieLStar,
///     PolarityMode::ActiveLow,
///     Duration::from_millis(12_800), // 12.8 s cycle
///     Duration::from_millis(50),     // 50 ms update interval
/// ).unwrap();
///
/// loop {
///     led.breathe().await.unwrap();
/// }
/// ```
pub struct BreathLed<P: SetDutyCycle, G: GammaMap = GammaCorrection> {
    led: PwmLed<P, G>,
    breath: Breath,
}

impl<P: SetDutyCycle, G: GammaMap> BreathLed<P, G> {
    /// Create a new breathing PWM LED and force it to the logical OFF state.
    ///
    /// The channel should already be enabled.  Guarantees the LED starts
    /// dark and the breath phase at the cycle start.
    ///
    /// See [`PwmLed::new`] and [`Breath::new`] for parameter details.
    pub fn new(
        pin: P,
        gamma: G,
        polarity: PolarityMode,
        cycle: embassy_time::Duration,
        interval: embassy_time::Duration,
    ) -> Result<Self, P::Error> {
        let led = PwmLed::new(pin, gamma, polarity)?;
        let breath = Breath::new(cycle, interval);
        Ok(Self { led, breath })
    }

    /// Reset the breathing cycle to the start (brightness minimum).
    ///
    /// Does **not** update the LED — call [`breathe`](Self::breathe)
    /// afterwards to apply.
    #[inline]
    pub fn reset_breath(&mut self) {
        self.breath.reset();
    }

    /// Advance one breath step, update LED, then sleep for the configured
    /// interval.
    ///
    /// This is the main animation method — combines brightness update with
    /// [`Timer::after`](embassy_time::Timer::after).
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     led.breathe().await.unwrap();
    /// }
    /// ```
    #[inline]
    pub async fn breathe(&mut self) -> Result<(), P::Error> {
        let brightness = self.breath.next();
        self.led.set_brightness(brightness)?;
        embassy_time::Timer::after(self.breath.interval).await;
        Ok(())
    }

    /// Return a reference to the inner [`PwmLed`].
    ///
    /// Use this escape hatch for non-breathing operations such as
    /// [`on`](PwmLed::on), [`off`](PwmLed::off), or direct brightness
    /// control.
    #[inline]
    pub fn led(&mut self) -> &mut PwmLed<P, G> {
        &mut self.led
    }

    /// Consume and return the underlying [`PwmLed`], discarding the
    /// breath state.
    #[inline]
    pub fn release(self) -> PwmLed<P, G> {
        self.led
    }
}

// ── Debug / defmt for BreathLed ───────────────────────

impl<P: SetDutyCycle, G: GammaMap> core::fmt::Debug for BreathLed<P, G>
where
    G: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BreathLed")
            .field("led", &self.led)
            .field("breath", &self.breath)
            .finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: SetDutyCycle, G: GammaMap> defmt::Format for BreathLed<P, G>
where
    G: defmt::Format,
{
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "BreathLed {{ led: {}, breath: {} }}",
            self.led,
            self.breath
        )
    }
}

// ── Tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use embassy_time::Duration;

    // ── cordic_sin ───────────────────────────────

    #[test]
    fn cordic_sin_range() {
        for phase in (0..=65535u16).step_by(137) {
            let v = cordic_sin(phase);
            let vu = brightness::to_u32(v);
            assert!(
                vu <= brightness::MAX_BRIGHTNESS,
                "cordic_sin({phase}) = {vu}, exceeds max {}",
                brightness::MAX_BRIGHTNESS
            );
        }
    }

    #[test]
    fn cordic_sin_endpoints() {
        let max = brightness::MAX_BRIGHTNESS;
        // sin(0) ≈ 0, mapped near the midpoint of the brightness range
        let mid = max / 2;
        let v0 = brightness::to_u32(cordic_sin(0));
        let half_range = if max == 255 { 2 } else { 32 };
        assert!(
            v0.abs_diff(mid) <= half_range,
            "sin(0) = {v0}, expected ~{mid}"
        );

        // sin(π/2) = sin(16384) ≈ 1 → max brightness
        let v90 = brightness::to_u32(cordic_sin(16384));
        assert!(
            v90 >= max.saturating_sub(2),
            "sin(π/2) = {v90}, expected ~{max}"
        );

        // sin(π) = sin(32768) ≈ 0 → midpoint
        let v180 = brightness::to_u32(cordic_sin(32768));
        assert!(
            v180.abs_diff(mid) <= half_range,
            "sin(π) = {v180}, expected ~{mid}"
        );

        // sin(3π/2) = sin(49152) ≈ -1 → near min
        let v270 = brightness::to_u32(cordic_sin(49152));
        let near_min = if max == 255 { 2 } else { 33 };
        assert!(v270 <= near_min, "sin(3π/2) = {v270}, expected ≤{near_min}");
    }

    #[test]
    fn cordic_sin_symmetry() {
        // sin(phase) and sin(π − phase) should be symmetric around midpoint
        let mid = brightness::MAX_BRIGHTNESS / 2;
        for phase in (0..=16384u16).step_by(257) {
            let a = brightness::to_u32(cordic_sin(phase));
            let b = brightness::to_u32(cordic_sin(32768 - phase));
            // Distance from midpoint
            let sym_a = a.abs_diff(mid);
            let sym_b = b.abs_diff(mid);
            let diff = sym_a.abs_diff(sym_b);
            let tolerance = if brightness::MAX_BRIGHTNESS == 255 {
                2
            } else {
                33
            };
            assert!(
                diff <= tolerance,
                "symmetry broken at phase={phase}: a={a}, b={b}"
            );
        }
    }

    #[test]
    fn cordic_sin_monotonic_rising() {
        // In [0, 16384] (0 to π/2), output should be non-decreasing
        let mut prev = brightness::to_u32(cordic_sin(0));
        for phase in 1..=16384u16 {
            let val = brightness::to_u32(cordic_sin(phase));
            assert!(
                val >= prev,
                "non-monotonic at phase={phase}: {prev} -> {val}"
            );
            prev = val;
        }
    }

    // ── Breath ────────────────────────────────────

    #[test]
    fn breath_new_does_not_panic() {
        let _b = Breath::new(Duration::from_millis(1000), Duration::from_millis(50));
    }

    #[test]
    fn breath_next_returns_valid_range() {
        let mut b = Breath::new(Duration::from_millis(1000), Duration::from_millis(50));
        for _ in 0..1000 {
            let v = brightness::to_u32(b.next());
            assert!(
                v <= brightness::MAX_BRIGHTNESS,
                "breath.next() = {v}, exceeds max"
            );
        }
    }

    #[test]
    fn breath_next_advances() {
        // A large interval ensures a big phase step, so consecutive calls
        // should return noticeably different brightness values.
        let mut b = Breath::new(Duration::from_millis(65536), Duration::from_millis(16384));
        let v1 = brightness::to_u32(b.next());
        let v2 = brightness::to_u32(b.next());
        let diff = v1.abs_diff(v2);
        assert!(diff > 0, "phase didn't advance: {v1}→{v2}");
    }

    #[test]
    fn breath_reset() {
        let mut b = Breath::new(Duration::from_millis(65536), Duration::from_millis(16384)); // 90° per step
        // Advance several steps away from the start
        for _ in 0..10 {
            b.next();
        }
        // Reset to cycle start
        b.reset();
        // After reset, next() should return near the starting (midpoint) brightness
        let v = brightness::to_u32(b.next());
        // sin(0) ≈ midpoint
        let mid = brightness::MAX_BRIGHTNESS / 2;
        let tolerance = if brightness::MAX_BRIGHTNESS == 255 {
            2
        } else {
            32
        };
        assert!(
            v.abs_diff(mid) <= tolerance * 2,
            "after reset got {v}, expected ~{mid}"
        );
    }

    #[test]
    fn breath_full_cycle_reaches_min_and_max() {
        // 256 steps per cycle — enough samples to hit both extremes
        let mut b = Breath::new(Duration::from_millis(65536), Duration::from_millis(256));
        let mut min = brightness::MAX_BRIGHTNESS;
        let mut max = 0u32;
        for _ in 0..300 {
            let v = brightness::to_u32(b.next());
            min = min.min(v);
            max = max.max(v);
        }
        let near_min = if brightness::MAX_BRIGHTNESS == 255 {
            2
        } else {
            33
        };
        assert!(min <= near_min, "min should be near 0, got {min}");
        assert!(
            max >= brightness::MAX_BRIGHTNESS.saturating_sub(2),
            "max should be near {}, got {max}",
            brightness::MAX_BRIGHTNESS
        );
    }

    #[test]
    #[should_panic(expected = "cycle duration must be > 0 ms")]
    fn breath_new_cycle_zero_panics() {
        Breath::new(Duration::from_millis(0), Duration::from_millis(50));
    }

    #[test]
    #[should_panic(expected = "interval duration must be > 0 ms")]
    fn breath_new_interval_zero_panics() {
        Breath::new(Duration::from_millis(1000), Duration::from_millis(0));
    }

    // ── BreathLed (requires pwm mock) ─────────────

    mod breath_led_tests {
        use super::*;
        use crate::PolarityMode;
        use crate::pwm::GammaCorrection;
        use embassy_time::Duration;
        use embedded_hal_mock::eh1::pwm::Mock as PwmMock;
        use embedded_hal_mock::eh1::pwm::Transaction as PwmTrans;

        const MAX_DUTY: u16 = 1000;

        #[test]
        fn breath_led_new_starts_off() {
            let e = [
                PwmTrans::max_duty_cycle(MAX_DUTY),
                PwmTrans::set_duty_cycle(0),
            ];
            let led = BreathLed::new(
                PwmMock::new(&e),
                GammaCorrection::Linear,
                PolarityMode::ActiveHigh,
                Duration::from_millis(10_000),
                Duration::from_millis(50),
            )
            .unwrap();
            assert!(led.led().is_off());
            led.release().release().done();
        }

        #[test]
        fn breath_led_reset_breath() {
            let e = [
                PwmTrans::max_duty_cycle(MAX_DUTY),
                PwmTrans::set_duty_cycle(0), // new → off
            ];
            let mut led = BreathLed::new(
                PwmMock::new(&e),
                GammaCorrection::Linear,
                PolarityMode::ActiveHigh,
                Duration::from_millis(10_000),
                Duration::from_millis(50),
            )
            .unwrap();
            // reset_breath() should not cause any PWM operations
            led.reset_breath();
            led.release().release().done();
        }
    }
}
