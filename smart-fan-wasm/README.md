# smart-fan-wasm

The [`smart-fan-cli`](../smart-fan-cli) in a browser tab. An iroh endpoint compiled
to WebAssembly that dials the [`smart-fan-esp32`](../smart-fan-esp32) firmware from
its ticket and shows temperature, humidity, and the fan state — the `GetStatus` RPC
over `SENSOR_ALPN` (from [`smart-fan-proto`](../smart-fan-proto)), polled every 10s.
With the device's `FAN_API_SECRET` entered, a slider sets the fan's temperature
threshold via the protected `SetThreshold` RPC.

Browsers can't open UDP/QUIC sockets, so this endpoint is **relay-only** (the `N0`
preset): it reaches the device through an n0 relay and resolves its address via pkarr.

As a small flourish, each fresh reading shows in mid-blue and fades to a readable
gray over ~30s — so a live link stays blue while a stalled one greys out.

## Build

Needs the wasm target and the [`wasm-bindgen` CLI](https://crates.io/crates/wasm-bindgen-cli)
matching the crate's pinned version:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126

npm run build        # release wasm + self-contained bundle; or `npm run build:debug`
```

`build` compiles the wasm once, writes the JS bindings to `public/wasm/`, then bundles
one **self-contained directory per variant** into `dist/` (from `variants/*.html` +
the shared `public/main.js`/`style.css`/`wasm/`): `remote-thermometer/` (readings
only), `smart-fan-readonly/` (+ fan state), and `smart-fan/` (+ threshold control).
Each is a drop-in `index.html` (CSS + JS inlined) plus its own `wasm/` copy. The
source under `public/` stays modular for development, and `npm run bundle` re-runs
just the bundling step.

### Deploying to iroh.computer

The blog embeds these via `<iframe>` on <https://iroh.computer>, which is a **separate
repo** — there's no shared build. After `npm run build`, copy the bundles by hand into
that repo, colocated with the post's other assets:

```bash
cp -R dist/* ../../iroh.computer/public/blog/an-iroh-powered-smart-fan/
# served at /blog/an-iroh-powered-smart-fan/{remote-thermometer,smart-fan-readonly,smart-fan}/
```

(Adjust the path to wherever the `iroh.computer` checkout lives.) `main.js` namespaces
`localStorage` by the last URL path segment, so the three can be embedded on one page
without sharing an endpoint identity.

## Run

```bash
npm run serve        # python3 -m http.server 8080 --directory dist
```

`serve` serves the built `dist/`, so run `npm run build` first. Open a variant at
<http://localhost:8080/smart-fan/> (or `/smart-fan-readonly/`, `/remote-thermometer/`),
paste the ticket from the firmware's serial output, and connect. The short id-only
ticket is enough — pkarr discovery resolves the rest.
To control the fan, enter the device's `FAN_API_SECRET` (from its serial log) to
unlock the threshold slider.

## License

MIT OR Apache-2.0, at your option.
