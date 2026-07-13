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
- **Optional breathing** — Sinusoidal brightness animation via `Breath`
  (CORDIC algorithm — zero float/mul/large tables) and `BreathLed` wrapper.
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
status-led = { version = "0.4", features = ["pwm"] }
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
status-led = { version = "0.5", features = ["breath"] }
```

`Breath` generates a stream of brightness values (0–255) following a
sinusoidal pattern.  Combine with `BreathLed` for a ready-to-use
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
| `defmt` | `defmt::Format` impls for all public types | `defmt` |

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
