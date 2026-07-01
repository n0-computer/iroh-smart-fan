# iroh-smart-fan

An [iroh](https://iroh.computer) echo endpoint running on a **PSRAM ESP32**, dialed
from a desktop CLI. Both speak standard QUIC, so a stock desktop iroh and the ESP32 talk to each other over the internet (via an n0 relay) or locally with nothing
in between.

This is the foundation for a smart-fan project; for now it's the smallest thing
worth shipping: **echo**.

## Two crates (deliberately not a workspace)

| crate | what |
|-------|------|
| [`smart-fan-cli/`](smart-fan-cli) | native CLI that dials the ticket and echoes a message |
| [`smart-fan-esp32/`](smart-fan-esp32) | the ESP32 firmware (echo server) |

They can't be one workspace: the firmware needs a different toolchain and a patched
iroh, while the client uses released iroh. Conflicting dependency graphs → two
standalone crates.

## Target boards

Anything ESP32 (Xtensa LX6) **with PSRAM** — e.g. an ESP32-WROVER (4 MiB) or an
M5StickC (2 MiB). PSRAM is what gives the iroh heap room to breathe; the firmware
uses iroh's default buffers, no frugal tuning.

## Quick start

1. **Flash the firmware** (needs the esp Rust toolchain — see [`smart-fan-esp32/`](smart-fan-esp32)):
   ```bash
   cd smart-fan-esp32
   WIFI_CONFIG='SSID:PASSWORD' cargo run --release
   ```
   It prints an **endpoint ticket** on the serial console.

2. **Run the client** with that ticket:
   ```bash
   cd smart-fan-cli
   cargo run -- <endpoint-ticket>
   ```
   You should see `Echo OK — iroh <-> ESP32!`.
