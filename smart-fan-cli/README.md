# smart-fan-cli

A minimal iroh client. Dials the [`smart-fan-esp32`](../smart-fan-esp32) firmware
from its ticket and asks for the latest sensor reading — one `GetLatest` RPC over
`SENSOR_ALPN` (from [`smart-fan-proto`](../smart-fan-proto)). The whole thing is
[~40 lines](src/main.rs).

## Run

```bash
cargo run -- <endpoint-ticket>
```

Get the ticket from the firmware's serial output (the **long** ticket). Expected:

```
Connecting to 03b43add965a…
Latest reading: 27.6°C  49.8%
```

(or `No reading yet — the sensor hasn't produced one.` if you dial before the first
successful read).

## License

MIT OR Apache-2.0, at your option.
