# status-led

[![CI](https://github.com/loongsoldier/status-led/actions/workflows/ci.yml/badge.svg)](https://github.com/loongsoldier/status-led/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/status-led.svg)](https://crates.io/crates/status-led)
[![docs.rs](https://docs.rs/status-led/badge.svg)](https://docs.rs/status-led)

`no_std` monochrome status LED abstraction for the [embassy] ecosystem.

- **Polarity** — `ActiveHigh` / `ActiveLow` chosen at construction time
  via [`PolarityMode`].
- **Reads hardware directly** — no internal state cache; `is_on()` reads
  the ODR register and applies polarity conversion.
- **Optional PWM** — CIE L\* perceptual brightness via `PwmLed` with a
  compact 32-byte lookup table + shift-add interpolation (zero multiply/divide).
  Supports 8-bit (0–255) and 12-bit (0–4095, via `arbitrary_int::u12`)
  brightness resolution.
- **Optional breathing** — Sinusoidal brightness animation via `Breath`
  (CORDIC algorithm — zero float/mul/large tables) and `BreathLed` wrapper,
  also respecting the selected brightness bit-depth.
- **Zero mandatory dependencies** — only `embedded-hal` 1.0.

[embassy]: https://embassy.dev

## Usage

```rust
use embassy_stm32::gpio::{Output, Level, Speed};
use status_led::{Led, PolarityMode};

let pin = Output::new(p.PA5, Level::High, Speed::Low);
let mut led = Led::from_pin(pin, PolarityMode::ActiveLow);

led.on().unwrap();
led.off().unwrap();
```

### PWM with gamma correction

Enable the `pwm` feature:

```toml
status-led = { version = "0.7", features = ["pwm"] }
```

```rust
use status_led::pwm::{GammaCorrection, PwmLed};
use status_led::PolarityMode;

let mut led = PwmLed::new(ch, GammaCorrection::CieLStar, PolarityMode::ActiveLow).unwrap();
led.set_brightness(128).unwrap(); // ~50% perceived brightness
```

### Breathing effect

Enable the `breath` feature:

```toml
status-led = { version = "0.7", features = ["breath"] }
```

`Breath` generates a stream of brightness values following a
sinusoidal pattern — 0–255 by default, or 0–4095 with the
`brightness-12bit` feature.  Combine with `BreathLed` for a ready-to-use
PWM LED wrapper:

```rust
use status_led::breath::{Breath, BreathLed};
use status_led::pwm::GammaCorrection;
use status_led::PolarityMode;

// Manual breath loop with Breath
let mut breath = Breath::new(Duration::from_millis(12_800), Duration::from_millis(50));
loop {
    led.set_brightness(breath.next()).unwrap();
    Timer::after_millis(50).await;
}

// Or use BreathLed for a single-call wrapper
let mut led = BreathLed::new(
    ch,
    GammaCorrection::CieLStar,
    PolarityMode::ActiveLow,
    Duration::from_millis(12_800),
    Duration::from_millis(50),
).unwrap();

loop {
    led.breathe().await.unwrap();
}
```

`breathe()` combines the brightness update with the sleep — no separate
`Timer` call needed.

### 12-bit brightness

Enable the `brightness-12bit` feature for finer brightness control:

```toml
status-led = { version = "0.7", features = ["breath", "brightness-12bit"] }
```

This switches the brightness type from `u8` (0–255) to
[`arbitrary_int::u12`](https://docs.rs/arbitrary-int) (0–4095), giving
16× finer resolution.  The CIE L\* gamma tables (still only 32 bytes)
are reused with two-level interpolation — no extra flash cost.

```rust
use status_led::pwm::{GammaCorrection, PwmLed};
use status_led::PolarityMode;

let mut led = PwmLed::new(ch, GammaCorrection::CieLStar, PolarityMode::ActiveLow).unwrap();
led.set_brightness(2048.try_into().unwrap()).unwrap(); // ~50% perceived brightness
```

The `u12` type guarantees values are always in the 0–4095 range at
compile time — out-of-range values panic immediately.

### Runtime polarity

When the polarity is read from configuration at runtime:

```rust
use status_led::{Led, PolarityMode};

let pol = if config.active_low {
    PolarityMode::ActiveLow
} else {
    PolarityMode::ActiveHigh
};
let mut led = Led::new(pin, pol).unwrap();
led.toggle().unwrap();
```

## Features

| Feature | Description | Extra deps |
|---|---|---|
| *(none)* | GPIO LED with runtime polarity | — |
| `pwm` | `PwmLed` with CIE L\* perceptual brightness, zero runtime multiply/divide | — |
| `breath` | `Breath` + `BreathLed` — CORDIC sinusoidal breathing with async `breathe()` | — (enables `pwm`, `embassy-time`) |
| `brightness-12bit` | Switches brightness type from `u8` to `arbitrary_int::u12` (0–4095) | `arbitrary-int` |
| `defmt` | `defmt::Format` impls for all public types | `defmt` |

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
