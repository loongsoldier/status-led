//! Sinusoidal breathing effect for monochrome LEDs.
//!
//! [`Breath`] generates a stream of brightness values (0–255) following a
//! sine wave, suitable for driving a PWM LED with a natural "breathing"
//! fade-in / fade-out.
//!
//! Enable the `breath` feature in `Cargo.toml`:
//!
//! ```toml
//! status-led = { version = "0.5", features = ["breath"] }
//! ```
//!
//! Combine with `pwm` for [`BreathLed`], a ready-to-use PWM LED wrapper:
//!
//! ```toml
//! status-led = { version = "0.5", features = ["breath", "pwm"] }
//! ```

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

/// Compute sin(phase) mapped to 0..=255.
///
/// Phase is a `u16` where 0 → 0°, 16384 → 90°, 32768 → 180°.
/// Uses CORDIC rotation mode with shift-add only — zero multiplication,
/// zero lookup tables (beyond the 30-byte atan table), zero floating
/// point.
#[inline]
fn cordic_sin(phase: u16) -> u8 {
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

    for i in 0..15 {
        let d: i32 = if angle >= 0 { 1 } else { -1 };
        let x_next = x - d * (y >> i);
        let y_next = y + d * (x >> i);
        angle -= d * CORDIC_ATAN[i] as i32;
        x = x_next;
        y = y_next;
    }

    if negate {
        y = -y;
    }

    // y ∈ [-8_388_608, 8_388_608], >>16 → [-128, 128], +128 → [0, 256], clamp
    ((y >> 16) + 128).clamp(0, 255) as u8
}

// ── Breath ────────────────────────────────────────────

/// Breathing effect generator.
///
/// Produces a sequence of brightness values (0–255) following a sinusoidal
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
/// let mut breath = Breath::new(12_800, 50);
///
/// loop {
///     led.set_brightness(breath.next()).unwrap();
///     Timer::after_millis(50).await;
/// }
/// ```
pub struct Breath {
    phase: u16,
    phase_step: u16,
    interval_ms: u32,
}

impl Breath {
    /// Create a new breathing generator.
    ///
    /// * `cycle_ms` — duration of one full breath cycle (bright → dark →
    ///   bright) in milliseconds.
    /// * `interval_ms` — time between [`next`](Self::next) calls in
    ///   milliseconds.  Should be ≤ `cycle_ms`.
    ///
    /// # Panics
    ///
    /// Panics if `cycle_ms == 0` or `interval_ms == 0`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 8-second cycle at 20 Hz → 400 steps
    /// let breath = Breath::new(8000, 50);
    /// ```
    #[track_caller]
    pub fn new(cycle_ms: u32, interval_ms: u32) -> Self {
        assert!(cycle_ms > 0, "cycle_ms must be > 0");
        assert!(interval_ms > 0, "interval_ms must be > 0");
        // phase_step = 65536 × interval_ms / cycle_ms
        let step = ((65536u64 * interval_ms as u64) / cycle_ms as u64) as u16;
        Self {
            phase: 0,
            phase_step: step.max(1),
            interval_ms,
        }
    }

    /// Advance the phase and return the next brightness value (0–255).
    ///
    /// The brightness follows a sinusoidal curve: starts near 0, rises to
    /// 255 at the peak, then falls back to near 0.
    #[inline]
    pub fn next(&mut self) -> u8 {
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
            .field("interval_ms", &self.interval_ms)
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
            self.interval_ms
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
/// status-led = { version = "0.5", features = ["breath"] }
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
///     12_800, // 12.8 s cycle
///     50,     // 50 ms update interval
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
        cycle_ms: u32,
        interval_ms: u32,
    ) -> Result<Self, P::Error> {
        let led = PwmLed::new(pin, gamma, polarity)?;
        let breath = Breath::new(cycle_ms, interval_ms);
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
    /// [`Timer::after_millis`](embassy_time::Timer::after_millis).
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
        embassy_time::Timer::after_millis(self.breath.interval_ms as u64).await;
        Ok(())
    }

    /// Return a reference to the inner [`PwmLed`].
    ///
    /// Use this escape hatch for non-breathing operations such as
    /// [`on`](PwmLed::on), [`off`](PwmLed::off), or direct brightness
    /// control.
    #[inline]
    pub fn led(&self) -> &PwmLed<P, G> {
        &self.led
    }

    /// Consume and return the underlying [`PwmLed`], discarding the
    /// breath state.
    #[inline]
    pub fn release(self) -> PwmLed<P, G> {
        self.led
    }
}

// ── Debug / defmt for BreathLed ───────────────────────

impl<P: SetDutyCycle, G: GammaMap> core::fmt::Debug for BreathLed<P, G> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BreathLed")
            .field("led", &self.led)
            .field("breath", &self.breath)
            .finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: SetDutyCycle, G: GammaMap> defmt::Format for BreathLed<P, G> {
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

    // ── cordic_sin ───────────────────────────────

    #[test]
    fn cordic_sin_range() {
        for phase in (0..=65535u16).step_by(137) {
            let _v = cordic_sin(phase);
            // cordic_sin always returns u8, so 0..=255 is guaranteed by type
        }
    }

    #[test]
    fn cordic_sin_endpoints() {
        // sin(0) ≈ 0, mapped to 128 (midpoint of [0,255] for sine's zero crossing)
        let v0 = cordic_sin(0);
        assert!(v0 >= 126 && v0 <= 130, "sin(0) = {}", v0);

        // sin(π/2) = sin(16384) ≈ 1, mapped to 255
        let v90 = cordic_sin(16384);
        assert!(v90 >= 253, "sin(π/2) = {}", v90);

        // sin(π) = sin(32768) ≈ 0, mapped to 128
        let v180 = cordic_sin(32768);
        assert!(v180 >= 126 && v180 <= 130, "sin(π) = {}", v180);

        // sin(3π/2) = sin(49152) ≈ -1, mapped to near 0
        let v270 = cordic_sin(49152);
        assert!(v270 <= 2, "sin(3π/2) = {}", v270);
    }

    #[test]
    fn cordic_sin_symmetry() {
        // sin(phase) and sin(π − phase) should be symmetric around 128
        for phase in (0..=16384u16).step_by(257) {
            let a = cordic_sin(phase);
            let b = cordic_sin(32768 - phase);
            let sym_a = (a as i16 - 128).unsigned_abs();
            let sym_b = (b as i16 - 128).unsigned_abs();
            let diff = (sym_a as i16 - sym_b as i16).unsigned_abs();
            assert!(
                diff <= 2,
                "symmetry broken at phase={}: a={}, b={}",
                phase,
                a,
                b
            );
        }
    }

    #[test]
    fn cordic_sin_monotonic_rising() {
        // In [0, 16384] (0 to π/2), output should be non-decreasing
        let mut prev = cordic_sin(0);
        for phase in 1..=16384u16 {
            let val = cordic_sin(phase);
            assert!(
                val >= prev,
                "non-monotonic at phase={}: {} -> {}",
                phase,
                prev,
                val
            );
            prev = val;
        }
    }

    // ── Breath ────────────────────────────────────

    #[test]
    fn breath_new_does_not_panic() {
        let _b = Breath::new(1000, 50);
    }

    #[test]
    fn breath_next_returns_valid_range() {
        let mut b = Breath::new(1000, 50);
        for _ in 0..1000 {
            let _v = b.next();
            // next() returns u8, so 0..=255 is guaranteed by type.
            // This test verifies no panic over many iterations.
        }
    }

    #[test]
    fn breath_next_advances() {
        // A large interval ensures a big phase step, so consecutive calls
        // should return noticeably different brightness values.
        let mut b = Breath::new(65536, 16384);
        let v1 = b.next();
        let v2 = b.next();
        let diff = (v1 as i16 - v2 as i16).unsigned_abs();
        assert!(diff > 0, "phase didn't advance: {}→{}", v1, v2);
    }

    #[test]
    fn breath_reset() {
        let mut b = Breath::new(65536, 16384); // 90° per step
        // Advance several steps away from the start
        for _ in 0..10 {
            b.next();
        }
        // Reset to cycle start
        b.reset();
        // After reset, next() should return near the starting brightness
        let v = b.next();
        // sin(0) ≈ 128 (±2)
        assert!(
            (126..=130).contains(&v),
            "after reset got {v}, expected ~128"
        );
    }

    #[test]
    fn breath_full_cycle_reaches_min_and_max() {
        // 256 steps per cycle — enough samples to hit both extremes
        let mut b = Breath::new(65536, 256);
        let mut min = 255u8;
        let mut max = 0u8;
        for _ in 0..300 {
            let v = b.next();
            min = min.min(v);
            max = max.max(v);
        }
        assert!(min <= 2, "min should be near 0, got {}", min);
        assert!(max >= 253, "max should be near 255, got {}", max);
    }

    #[test]
    #[should_panic(expected = "cycle_ms must be > 0")]
    fn breath_new_cycle_zero_panics() {
        Breath::new(0, 50);
    }

    #[test]
    #[should_panic(expected = "interval_ms must be > 0")]
    fn breath_new_interval_zero_panics() {
        Breath::new(1000, 0);
    }

    // ── BreathLed (requires pwm mock) ─────────────

    mod breath_led_tests {
        use super::*;
        use crate::PolarityMode;
        use crate::pwm::GammaCorrection;
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
                10_000,
                50,
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
                10_000,
                50,
            )
            .unwrap();
            // reset_breath() should not cause any PWM operations
            led.reset_breath();
            led.release().release().done();
        }
    }
}
