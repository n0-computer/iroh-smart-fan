# smart-fan-cli

A minimal iroh **echo client**. Dials the [`smart-fan-esp32`](../smart-fan-esp32)
firmware from its ticket, sends a message, and checks it comes back — the whole
thing is [~50 lines](src/main.rs).

## Run

```bash
cargo run -- <endpoint-ticket>
```

Get the ticket from the firmware's serial output. Expected:

```
Sent:     Hello from iroh!
Received: Hello from iroh!
Echo OK — iroh <-> ESP32!
```

## License

MIT OR Apache-2.0, at your option.
