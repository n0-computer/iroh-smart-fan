# smart-fan-esp32 — iroh echo firmware

An [iroh](https://iroh.computer) endpoint running on a **PSRAM ESP32**. It joins
WiFi, binds an iroh endpoint with n0 relays + pkarr discovery, prints a ticket, and
serves the `echo/0` protocol — bytes in, same bytes out. Dial it with the
[`smart-fan-cli`](../smart-fan-cli).

Runs on any ESP32 (Xtensa LX6) with PSRAM: an ESP32-WROVER (4 MiB) or an
M5StickC (2 MiB). PSRAM holds the malloc heap, so iroh's default buffers fit — no
frugal tuning ([`sdkconfig.defaults`](sdkconfig.defaults) is short).

## Layout

The endpoint is ordinary iroh — a `Router` accepting `echo/0` with a one-line
`tokio::io::copy` handler ([`src/main.rs`](src/main.rs)). The extra modules are the
ESP32 platform glue:

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
