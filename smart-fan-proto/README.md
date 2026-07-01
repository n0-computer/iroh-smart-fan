# smart-fan-proto

The shared [irpc](https://docs.rs/irpc) protocol between the
[`smart-fan-esp32`](../smart-fan-esp32) firmware (server) and the
[`smart-fan-cli`](../smart-fan-cli) (client). It defines:

- **`Reading`** — one sensor sample (`temperature`, `humidity`).
- **`SENSOR_ALPN`** — the ALPN the firmware serves and the client dials.
- **`SensorProtocol`** — the RPC surface. One call for now:
  `GetLatest → Option<Reading>`.

Deliberately **board-agnostic**: no esp-idf dependencies and **no `[patch]`**, so it
builds on the host (as a dependency of the CLI) and on `xtensa-esp32-espidf` (as a
dependency of the firmware). The firmware's `[patch.crates-io]` — which redirects
`irpc` to a ring-free build — applies graph-wide, so this crate's published `irpc`
unifies onto it automatically when built for the ESP32.

Both crates depend on it by path, which is why the three live in one repo but not
one workspace.

## License

MIT OR Apache-2.0, at your option.
