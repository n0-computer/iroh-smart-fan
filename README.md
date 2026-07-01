# iroh-smart-fan

An [iroh](https://iroh.computer) endpoint running on a **PSRAM ESP32** that reads a
DHT22 temperature/humidity sensor and serves the readings over standard QUIC. A
desktop CLI dials it — over the internet (via an n0 relay) or locally, with nothing
in between — and asks for the latest reading.

Foundation for a smart-fan project. (It also still answers the `echo/0` protocol
from the initial bring-up.)

## Three crates (deliberately not a workspace)

| crate | what |
|-------|------|
| [`smart-fan-proto/`](smart-fan-proto) | the shared irpc protocol: `Reading` + a `GetLatest` RPC |
| [`smart-fan-esp32/`](smart-fan-esp32) | the ESP32 firmware — reads the DHT22 and serves the RPC |
| [`smart-fan-cli/`](smart-fan-cli) | native CLI that dials the ticket and fetches the latest reading |

The firmware and CLI can't share a workspace: the firmware needs a different
toolchain and a patched iroh, while the client uses released iroh. The proto crate
is board-agnostic (no `[patch]`) and is a path dependency of both.

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
   It prints an **endpoint ticket** on the serial console (use the **long** ticket).

2. **Run the client** with that ticket:
   ```bash
   cd smart-fan-cli
   cargo run -- <endpoint-ticket>
   ```
   ```
   Latest reading: 27.6°C  49.8%
   ```
