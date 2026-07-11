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
| `defmt` | `defmt::Format` impls for all public types | `defmt` |

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
