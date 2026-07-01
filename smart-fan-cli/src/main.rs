//! Minimal iroh client for the smart-fan ESP32 firmware.
//!
//! Dials the endpoint from its ticket and asks for the latest sensor reading — one
//! `GetLatest` RPC over `SENSOR_ALPN`.
//!
//! Usage:
//!     cargo run -- <endpoint-ticket>

use anyhow::Context;
use iroh::endpoint::presets;
use iroh::Endpoint;
use iroh_tickets::endpoint::EndpointTicket;
use irpc::Client;
use irpc_iroh::IrohRemoteConnection;
use smart_fan_proto::{GetLatest, SensorProtocol, SENSOR_ALPN};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // iroh's TLS uses `ring` on the desktop; install its provider before we build.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install ring crypto provider");

    let ticket: EndpointTicket = std::env::args()
        .nth(1)
        .context("usage: smart-fan-cli <endpoint-ticket>")?
        .parse()?;
    let addr: iroh::EndpointAddr = ticket.into();

    // `N0` preset = n0 relays + pkarr discovery, matching the firmware.
    let endpoint = Endpoint::builder(presets::N0).bind().await?;

    println!("Connecting to {}…", addr.id);
    let conn = endpoint.connect(addr, SENSOR_ALPN).await?;

    // Wrap the QUIC connection as an irpc client and make one call.
    let client: Client<SensorProtocol> = Client::boxed(IrohRemoteConnection::new(conn));
    match client.rpc(GetLatest).await? {
        Some(r) => println!("Latest reading: {:.1}°C  {:.1}%", r.temperature, r.humidity),
        None => println!("No reading yet — the sensor hasn't produced one."),
    }

    endpoint.close().await;
    Ok(())
}
