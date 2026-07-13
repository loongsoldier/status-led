//! Brightness type abstraction, switched by the `brightness-12bit` feature.
//!
//! - **Default (no feature):** `Brightness = u8`, range 0–255.
//! - **`brightness-12bit`:** `Brightness = arbitrary_int::u12`, range 0–4095.
//!
//! All PWM and breath code goes through the helpers in this module, so
//! the rest of the crate never needs `#[cfg]` on brightness bit-depth.

// ── 8-bit (default) ────────────────────────────────────

#[cfg(not(feature = "brightness-12bit"))]
mod inner {
    /// Brightness value — `u8` when the `brightness-12bit` feature is **not** set.
    pub type Brightness = u8;

    /// Logical maximum brightness (used for duty-scale precomputation).
    pub const MAX_BRIGHTNESS: u32 = 255;

    /// Numerator for the shift-based duty-scale precomputation:
    /// `duty_scale = max_duty * DUTY_SCALE_NUM`.
    pub const DUTY_SCALE_NUM: u32 = 257; // 65536/255 ≈ 257.004

    /// Convert a brightness value to `u32` for arithmetic.
    #[inline]
    pub fn to_u32(b: Brightness) -> u32 {
        b as u32
    }

    /// Clamp a `u32` into the valid brightness range.
    #[inline]
    pub fn from_u32_clamped(v: u32) -> Brightness {
        v.min(MAX_BRIGHTNESS) as u8
    }

    /// Convert a percentage (0–100) to the nearest brightness value.
    #[inline]
    pub fn from_percent(pct: u8) -> Brightness {
        let p = pct.min(100) as u32;
        let x = (p << 8).wrapping_sub(p).wrapping_add(50);
        (x.wrapping_add(x << 3).wrapping_add(x << 5) >> 12) as u8
    }

    /// The maximum possible brightness value.
    #[inline]
    pub fn max_value() -> Brightness {
        Brightness::MAX
    }

    /// The minimum possible brightness value (always 0).
    #[inline]
    pub fn min_value() -> Brightness {
        Brightness::default()
    }
}

// ── 12-bit (feature = "brightness-12bit") ──────────────

#[cfg(feature = "brightness-12bit")]
mod inner {
    use arbitrary_int::prelude::*;
    use arbitrary_int::u12;

    /// Brightness value — `arbitrary_int::u12` when `brightness-12bit` is set.
    pub type Brightness = u12;

    /// Logical maximum brightness (used for duty-scale precomputation).
    pub const MAX_BRIGHTNESS: u32 = 4095;

    /// Numerator for the shift-based duty-scale precomputation:
    /// `duty_scale = max_duty * DUTY_SCALE_NUM`.
    /// 65536/4095 ≈ 16.004 → 16 (error ≤ 0.024 %, imperceptible for LEDs).
    pub const DUTY_SCALE_NUM: u32 = 16;

    /// Convert a brightness value to `u32` for arithmetic.
    #[inline]
    pub fn to_u32(b: Brightness) -> u32 {
        u16::from(b) as u32
    }

    /// Clamp a `u32` into the valid brightness range.
    #[inline]
    pub fn from_u32_clamped(v: u32) -> Brightness {
        u12::new((v.min(MAX_BRIGHTNESS)) as u16)
    }

    /// Convert a percentage (0–100) to the nearest brightness value.
    #[inline]
    pub fn from_percent(pct: u8) -> Brightness {
        let p = pct.min(100) as u32;
        let v = (p * MAX_BRIGHTNESS + 50) / 100;
        u12::new(v as u16)
    }

    /// The maximum possible brightness value.
    #[inline]
    pub fn max_value() -> Brightness {
        Brightness::MAX
    }

    /// The minimum possible brightness value (always 0).
    #[inline]
    pub fn min_value() -> Brightness {
        Brightness::default()
    }
}

// ── Re-export ──────────────────────────────────────────

pub(crate) use inner::*;
