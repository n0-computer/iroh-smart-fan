# smart-fan-esp32 — iroh firmware (echo + DHT22)

An [iroh](https://iroh.computer) endpoint running on a **PSRAM ESP32**, with a
**DHT22** temperature/humidity sensor. It joins WiFi, binds an iroh endpoint (n0
relays + pkarr discovery) serving the `echo/0` protocol — dial it with the
[`smart-fan-cli`](../smart-fan-cli) — and, on a separate thread, reads the DHT22 and
logs it.

> The sensor readings are **not exposed over iroh yet** — for now they just print to
> the serial console. Surfacing them over an irpc service is the next step.

Runs on any ESP32 (Xtensa LX6) with PSRAM: an ESP32-WROVER (4 MiB) or an
M5StickC (2 MiB). PSRAM holds the malloc heap, so iroh's default buffers fit — no
frugal tuning ([`sdkconfig.defaults`](sdkconfig.defaults) is short).

## Sensor

A DHT22 is wired to **GPIO26** (data), **3.3 V** (not 5 V), and GND, with a
**4.7 kΩ–10 kΩ pull-up** on the data line. A dedicated thread bit-bangs the
single-wire protocol (`read_dht22` in [`src/main.rs`](src/main.rs)) every 2 s and
prints, e.g.:

```
I (288685) smart_fan_esp32: DHT22: 27.6°C  49.8%
```

Reads that get preempted by WiFi/QUIC retry once; a persistent failure logs
`DHT22 read failed: timeout`.

## Layout

The endpoint is ordinary iroh — a `Router` accepting `echo/0` with a one-line
`tokio::io::copy` handler ([`src/main.rs`](src/main.rs)); the sensor runs on its own
thread. The extra modules are the ESP32 platform glue:

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

On startup it prints two tickets:

- **long ticket** — carries the endpoint's address; dial this.
- **short ticket** — just the endpoint ID; needs discovery to resolve.

## License

MIT OR Apache-2.0, at your option.
