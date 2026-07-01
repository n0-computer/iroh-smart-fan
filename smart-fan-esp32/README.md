# smart-fan-esp32 — iroh firmware (DHT22 sensor + echo)

An [iroh](https://iroh.computer) endpoint running on a **PSRAM ESP32** with a
**DHT22** temperature/humidity sensor. It joins WiFi, binds an iroh endpoint (n0
relays + pkarr discovery), reads the DHT22 on a dedicated thread, and serves the
latest reading over a small irpc service — dial it with the
[`smart-fan-cli`](../smart-fan-cli). It also still answers the `echo/0` protocol.

Runs on any ESP32 (Xtensa LX6) with PSRAM: an ESP32-WROVER (4 MiB) or an
M5StickC (2 MiB). PSRAM holds the malloc heap, so iroh's default buffers fit — no
frugal tuning ([`sdkconfig.defaults`](sdkconfig.defaults) is short).

## Sensor

A DHT22 is wired to **GPIO26** (data), **3.3 V** (not 5 V), and GND, with a
**4.7 kΩ–10 kΩ pull-up** on the data line. A dedicated thread bit-bangs the
single-wire protocol (`read_dht22` in [`src/main.rs`](src/main.rs)) every 2 s, logs
it, and stores it as the "latest" reading:

```
I (288685) smart_fan_esp32: DHT22: 27.6°C  49.8%
```

Reads that get preempted by WiFi/QUIC retry once; a persistent failure logs
`DHT22 read failed: timeout`.

## Protocol

The endpoint accepts two ALPNs on one [`Router`](src/main.rs):

- **`smart-fan/sensor/0`** — an [irpc](https://docs.rs/irpc) service with a single
  `GetLatest` call returning the most recent `Reading` (`Option`, `None` until the
  first read). The wire types live in the shared [`smart-fan-proto`](../smart-fan-proto)
  crate; the `SensorServer` handler answers each request from the shared latest slot.
- **`echo/0`** — the original bytes-in/bytes-out handler, kept from bring-up.

## Layout

The extra modules are the ESP32 platform glue:

- [`quic_crypto_provider.rs`](src/quic_crypto_provider.rs) — a pure-Rust rustls
  crypto provider (X25519 + AES-128-GCM).
- [`insecure_verifier.rs`](src/insecure_verifier.rs) — a server cert verifier that
  skips real-CA checks.
- [`std_dns_resolver.rs`](src/std_dns_resolver.rs) — DNS over std sockets.

## Build & flash

Needs the **esp Rust toolchain** ([`espup`](https://github.com/esp-rs/espup);
[`rust-toolchain.toml`](rust-toolchain.toml) selects it) and
[`espflash`](https://github.com/esp-rs/espflash) (the cargo runner). Connect the
board over USB, then:

```bash
WIFI_CONFIG='SSID:PASSWORD' cargo run --release
```

`WIFI_CONFIG` (SSID + password, colon-separated) is read at build time and baked in.
`IROH_SECRET` is also baked at build time — an env var if you set one, otherwise a
random key generated and cached by [`build.rs`](build.rs) — so the endpoint ID (and
ticket) stays **stable across reboots**.

On startup it prints two tickets (dial with the **long** one) and the sensor ALPN.

## License

MIT OR Apache-2.0, at your option.
