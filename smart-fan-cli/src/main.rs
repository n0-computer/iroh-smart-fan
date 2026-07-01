//! CLI for the smart-fan ESP32 firmware.
//!
//! Two subcommands:
//!   latest <ticket>  — fetch the most recent sensor reading (GetLatest RPC)
//!   echo   <ticket>  — round-trip a message off the device (echo protocol)

use clap::{Parser, Subcommand};
use iroh::endpoint::presets;
use iroh::Endpoint;
use iroh_tickets::endpoint::EndpointTicket;
use irpc::Client;
use irpc_iroh::IrohRemoteConnection;
use smart_fan_proto::{GetLatest, SensorProtocol, SENSOR_ALPN};

/// Must match the firmware's echo ALPN.
const ECHO_ALPN: &[u8] = b"echo/0";

#[derive(Parser)]
#[command(about = "Client for the smart-fan ESP32 firmware")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch the most recent sensor reading.
    Latest {
        /// Endpoint ticket printed by the firmware.
        ticket: String,
    },
    /// Round-trip a message off the device (echo protocol).
    Echo {
        /// Endpoint ticket printed by the firmware.
        ticket: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // iroh's TLS uses `ring` on the desktop; install its provider before we build.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install ring crypto provider");

    let cli = Cli::parse();

    // `N0` preset = n0 relays + pkarr discovery, matching the firmware.
    let endpoint = Endpoint::builder(presets::N0).bind().await?;

    match cli.command {
        Command::Latest { ticket } => {
            let addr: iroh::EndpointAddr = ticket.parse::<EndpointTicket>()?.into();
            println!("Connecting to {}…", addr.id);
            let conn = endpoint.connect(addr, SENSOR_ALPN).await?;
            let client: Client<SensorProtocol> = Client::boxed(IrohRemoteConnection::new(conn));
            match client.rpc(GetLatest).await? {
                Some(r) => println!("Latest reading: {:.1}°C  {:.1}%", r.temperature, r.humidity),
                None => println!("No reading yet — the sensor hasn't produced one."),
            }
        }
        Command::Echo { ticket } => {
            let addr: iroh::EndpointAddr = ticket.parse::<EndpointTicket>()?.into();
            println!("Connecting to {}…", addr.id);
            let conn = endpoint.connect(addr, ECHO_ALPN).await?;
            let (mut send, mut recv) = conn.open_bi().await?;
            let msg = b"Hello from iroh!";
            send.write_all(msg).await?;
            send.finish()?;
            let echoed = recv.read_to_end(64 * 1024).await?;
            anyhow::ensure!(echoed == msg, "echo mismatch!");
            println!("Echo OK: {}", String::from_utf8_lossy(&echoed));
            conn.close(0u32.into(), b"done");
        }
    }

    endpoint.close().await;
    Ok(())
}
