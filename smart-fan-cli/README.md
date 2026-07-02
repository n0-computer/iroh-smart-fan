# smart-fan-cli

A small iroh client for the [`smart-fan-esp32`](../smart-fan-esp32) firmware. It dials
the device from its ticket and speaks the `SENSOR_ALPN` irpc service (from
[`smart-fan-proto`](../smart-fan-proto)). Get the ticket from the firmware's serial
output — the short ticket works globally via discovery once the device has a home relay.

## Subcommands

```bash
cargo run -- latest <ticket>                    # one reading (GetLatest)
cargo run -- status <ticket>                    # reading + fan state + threshold (GetStatus)
cargo run -- set-threshold <ticket> 23 \        # set the fan threshold (SetThreshold)
    --secret <FAN_API_SECRET>                   #   or via $FAN_API_SECRET
cargo run -- echo <ticket>                      # round-trip a message (echo protocol)
cargo run -- qr <ticket> [--base URL] [--secret S] [--out qr.png]   # QR-code PNG for the URL
```

Example:

```
Connecting to 03b43add965a…
27.6°C  49.8%  fan off  (threshold 25°C)
```

`set-threshold` reports `Ok`, `Rejected — wrong FAN_API_SECRET.`, or the out-of-range
message. The `FAN_API_SECRET` is printed on the device's serial log at startup.

### `qr` — a scannable link to the device

`qr` is offline (no network): it encodes `<base>#ticket=<ticket>[&secret=<secret>]` as a
PNG. The GUI auto-connects when opened with a ticket in the URL, so printing the QR and
sticking it on the device lets you open its panel by scanning with a phone. Everything
goes in the URL **fragment** (`#…`), never the query — the fragment survives a static
host's trailing-slash redirect and never reaches the server (essential for the secret).
Use the **short** ticket — it's stable across reboots, so the QR stays valid for life.

Two QR codes, two trust levels:

```bash
# View-only — safe to display openly (even on the device):
cargo run -- qr <ticket> \
    --base https://iroh.computer/blog/an-iroh-powered-smart-fan/smart-fan-readonly \
    --out view.png

# Control — treat like a key; whoever scans it can drive the fan:
cargo run -- qr <ticket> --secret <FAN_API_SECRET> --out control.png
```

The secret is embedded in the fragment only, so it stays out of server logs — but it's
still in the QR image, browser history, and the scanning phone's localStorage, so guard
the control QR accordingly.

## License

MIT OR Apache-2.0, at your option.
