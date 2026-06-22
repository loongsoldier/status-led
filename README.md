# status-led

[![CI](https://github.com/loongsoldier/status-led/actions/workflows/ci.yml/badge.svg)](https://github.com/loongsoldier/status-led/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/status-led.svg)](https://crates.io/crates/status-led)
[![docs.rs](https://docs.rs/status-led/badge.svg)](https://docs.rs/status-led)

`no_std` monochrome status LED abstraction for the [embassy] ecosystem.

- **Type-safe polarity** — `ActiveHigh` / `ActiveLow` markers prevent
  logic errors at compile time.
- **Reads hardware directly** — no internal state cache; `is_on()` reads
  the ODR register and applies polarity conversion.
- **Optional PWM** — gamma-corrected brightness via `PwmLed` with a
  compact 16+16 byte lookup table.
- **`FlexLed` / `FlexPwmLed`** — runtime polarity when the configuration
  comes from a config file instead of a type parameter.
- **Zero mandatory dependencies** — only `embedded-hal` 1.0.

[embassy]: https://embassy.dev

## Usage

```rust
use embassy_stm32::gpio::{Output, Level, Speed};
use status_led::{Led, ActiveLow};

let pin = Output::new(p.PA5, Level::High, Speed::Low);
let mut led = Led::<_, ActiveLow>::from_pin(pin);

led.on().unwrap();
led.off().unwrap();
```

### PWM with gamma correction

Enable the `pwm` feature:

```toml
status-led = { version = "0.1", features = ["pwm"] }
```

```rust
use status_led::pwm::{PwmLed, GammaCorrection};
use status_led::ActiveLow;

let mut led = PwmLed::<_, ActiveLow>::new(ch, ch.max_duty_cycle(), GammaCorrection::SRGB);
led.set_brightness(128); // ~50% perceived brightness
```

### Runtime polarity

When the polarity is read from configuration at runtime:

```rust
use status_led::{FlexLed, PolarityMode};

let pol = if config.active_low {
    PolarityMode::ActiveLow
} else {
    PolarityMode::ActiveHigh
};
let mut led = FlexLed::new(pin, pol).unwrap();
led.toggle().unwrap();
```

## Features

| Feature | Description | Extra deps |
|---|---|---|
| *(none)* | GPIO LED with compile-time polarity | — |
| `pwm` | `PwmLed` + `FlexPwmLed` with gamma-corrected brightness | — |
| `defmt` | `defmt::Format` impls for all public types | `defmt` |

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
