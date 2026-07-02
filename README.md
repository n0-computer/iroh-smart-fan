# iroh-smart-fan

An [iroh](https://iroh.computer) endpoint running on a **PSRAM ESP32** that reads a
DHT22 temperature/humidity sensor, drives a fan from a temperature threshold, and
serves it all over standard QUIC. Dial it — over the internet (via an n0 relay) or
locally, with nothing in between — from a desktop **CLI** or a **browser GUI** (Rust
compiled to WebAssembly).

(It also still answers the `echo/0` protocol from the initial bring-up.)

## Four crates (deliberately not a workspace)

| crate | what |
|-------|------|
| [`smart-fan-proto/`](smart-fan-proto) | shared irpc protocol: `Reading`/`Status` + `GetLatest`, `GetStatus`, `SetThreshold` |
| [`smart-fan-esp32/`](smart-fan-esp32) | ESP32 firmware — reads the DHT22, drives the fan, serves the RPCs |
| [`smart-fan-cli/`](smart-fan-cli) | native CLI: fetch readings/status, set the threshold, make a setup QR |
| [`smart-fan-wasm/`](smart-fan-wasm) | browser GUI (Rust → WebAssembly): the CLI in a tab, relay-only |

The firmware can't share a workspace with the others: it needs a different toolchain
and a patched iroh, while the CLI and GUI use released iroh. The proto crate is
board-agnostic (no `[patch]`) and a path dependency of all three.

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
   It prints an **endpoint ticket** on the serial console (the short ticket works
   globally via discovery), plus a `FAN_API_SECRET` — the key needed to change the fan
   threshold.

2. **From the terminal** — fetch a reading / the full status, or set the threshold:
   ```bash
   cd smart-fan-cli
   cargo run -- status <endpoint-ticket>
   # 27.6°C  49.8%  fan off  (threshold 25°C)
   cargo run -- set-threshold <endpoint-ticket> 23 --secret <FAN_API_SECRET>
   ```

3. **Or in the browser** — the same, as a WebAssembly GUI:
   ```bash
   cd smart-fan-wasm
   npm run build && npm run serve
   ```
   Open <http://localhost:8080/smart-fan/>, paste the ticket, and — with the secret
   entered — drag the threshold slider. Browsers are relay-only; the short ticket is
   enough. See [`smart-fan-wasm/`](smart-fan-wasm) for the hosted-page and QR-code flow.
