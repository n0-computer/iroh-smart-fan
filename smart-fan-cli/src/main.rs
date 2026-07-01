//! Minimal iroh echo client for the PSRAM ESP32 firmware.
//!
//! Dials the endpoint from its ticket, opens one bidirectional stream, sends a
//! message, and checks it comes back echoed.
//!
//! Usage:
//!     cargo run -- <endpoint-ticket>

use anyhow::Context;
use iroh::endpoint::presets;
use iroh::Endpoint;
use iroh_tickets::endpoint::EndpointTicket;

/// Must match the firmware's `ECHO_ALPN`.
const ECHO_ALPN: &[u8] = b"echo/0";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // iroh's TLS uses `ring` on the desktop; install its provider before we build.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install ring crypto provider");

    let ticket: EndpointTicket = std::env::args()
        .nth(1)
        .context("usage: smart-fan-client <endpoint-ticket>")?
        .parse()?;
    let addr: iroh::EndpointAddr = ticket.into();

    // `N0` preset = n0 relays + pkarr discovery, matching the firmware.
    let endpoint = Endpoint::builder(presets::N0).bind().await?;

    println!("Connecting to {}…", addr.id);
    let conn = endpoint.connect(addr, ECHO_ALPN).await?;

    let (mut send, mut recv) = conn.open_bi().await?;
    let msg = b"Hello from iroh!";
    send.write_all(msg).await?;
    send.finish()?;
    println!("Sent:     {}", String::from_utf8_lossy(msg));

    // Read until the server closes its send side.
    let echoed = recv.read_to_end(64 * 1024).await?;
    println!("Received: {}", String::from_utf8_lossy(&echoed));
    anyhow::ensure!(echoed == msg, "echo mismatch!");
    println!("Echo OK — iroh <-> ESP32!");

    conn.close(0u32.into(), b"done");
    endpoint.close().await;
    Ok(())
}
