//! Browser GUI for the smart-fan ESP32: an iroh endpoint compiled to WebAssembly
//! that polls the device's latest sensor reading over the relay.
//!
//! Browsers can't open UDP/QUIC sockets, so this endpoint is relay-only (the `N0`
//! preset) — it reaches the PSRAM firmware via an n0 relay. It speaks the exact
//! same `SENSOR_ALPN` irpc service (`smart_fan_proto`) as the native CLI.

use std::cell::Cell;
use std::rc::Rc;

use iroh_tickets::endpoint::EndpointTicket;
use irpc::Client;
use irpc_iroh::IrohLazyRemoteConnection;
use smart_fan_proto::{GetStatus, SensorProtocol, SENSOR_ALPN};
use tracing::level_filters::LevelFilter;
use tracing_subscriber_wasm::MakeConsoleWriter;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// How often to re-fetch the latest reading.
const POLL_INTERVAL_MS: u32 = 10_000;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
    // Surface iroh's logs (incl. relay connection) in the browser console.
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .with_writer(MakeConsoleWriter::default().map_trace_level_to(tracing::Level::DEBUG))
        .without_time()
        .with_ansi(false)
        .init();
}

/// An iroh endpoint running in the browser.
#[wasm_bindgen]
pub struct Node {
    endpoint: iroh::Endpoint,
    secret_hex: String,
}

#[wasm_bindgen]
impl Node {
    /// Spawn the endpoint. Pass a hex secret key to keep a stable id across reloads,
    /// or `null`/empty to generate a fresh one.
    pub async fn spawn(secret: Option<String>) -> Result<Node, JsError> {
        let secret_key = match secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(hex) => hex.parse::<iroh::SecretKey>().map_err(js_err)?,
            None => iroh::SecretKey::generate(),
        };
        let secret_hex = hex_encode(&secret_key.to_bytes());
        let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(secret_key)
            .bind()
            .await
            .map_err(js_err)?;
        Ok(Node {
            endpoint,
            secret_hex,
        })
    }

    pub fn endpoint_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    pub fn secret_hex(&self) -> String {
        self.secret_hex.clone()
    }

    /// Connect to `ticket` and poll the device. `on_reading(temperature, humidity)`
    /// fires on each successful read; `on_status(text)` reports the connection state
    /// and any errors. Returns a [`Subscription`]; drop it (JS `.free()`) to stop
    /// the loop and close the connection — do that before subscribing again.
    pub fn subscribe(
        &self,
        ticket: String,
        on_reading: js_sys::Function,
        on_status: js_sys::Function,
    ) -> Subscription {
        let endpoint = self.endpoint.clone();
        let cancelled = Rc::new(Cell::new(false));
        let flag = cancelled.clone();
        spawn_local(async move {
            if let Err(err) = run(endpoint, ticket, &on_reading, &on_status, &flag).await {
                if !flag.get() {
                    status(&on_status, &format!("error: {err}"));
                }
            }
        });
        Subscription { cancelled }
    }
}

/// Handle to a running [`Node::subscribe`] poll loop. Dropping it (JS `.free()`)
/// signals the loop to stop and lets its connection close.
#[wasm_bindgen]
pub struct Subscription {
    cancelled: Rc<Cell<bool>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.cancelled.set(true);
    }
}

/// `IrohLazyRemoteConnection` reconnects under the hood if the relay link drops,
/// so a single client survives transient outages.
async fn run(
    endpoint: iroh::Endpoint,
    ticket: String,
    on_reading: &js_sys::Function,
    on_status: &js_sys::Function,
    cancelled: &Rc<Cell<bool>>,
) -> Result<(), String> {
    let ticket: EndpointTicket = ticket.trim().parse().map_err(|e| format!("bad ticket: {e}"))?;
    let addr: iroh::EndpointAddr = ticket.into();
    let conn = IrohLazyRemoteConnection::new(endpoint, addr, SENSOR_ALPN.to_vec());
    let client: Client<SensorProtocol> = Client::boxed(conn);

    status(on_status, "connecting…");
    loop {
        if cancelled.get() {
            return Ok(());
        }
        // One call gets everything: reading + fan state.
        let result = client.rpc(GetStatus).await;
        // Bail before touching the UI if we were superseded while awaiting, so a
        // late poll can't clobber the next subscription's display.
        if cancelled.get() {
            return Ok(());
        }
        match result {
            Ok(Some(s)) => {
                let _ = on_reading.call3(
                    &JsValue::NULL,
                    &JsValue::from_f64(s.reading.temperature as f64),
                    &JsValue::from_f64(s.reading.humidity as f64),
                    &JsValue::from_bool(s.fan),
                );
                status(on_status, "live");
            }
            Ok(None) => status(on_status, "no reading yet — waiting…"),
            Err(e) => status(on_status, &format!("rpc error: {e}")),
        }
        gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
    }
}

fn status(f: &js_sys::Function, text: &str) {
    let _ = f.call1(&JsValue::NULL, &JsValue::from_str(text));
}

fn js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&err.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
