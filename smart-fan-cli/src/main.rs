//! CLI for the smart-fan ESP32 firmware.
//!
//! Subcommands:
//!   latest <ticket>         — fetch the most recent sensor reading (GetLatest RPC)
//!   status <ticket>         — fetch the full status (GetStatus RPC)
//!   set-threshold <ticket>  — set the fan temperature threshold (SetThreshold RPC)
//!   echo   <ticket>         — round-trip a message off the device (echo protocol)
//!   qr     <ticket>         — write a QR-code PNG for the control URL (offline)

use clap::{Parser, Subcommand};
use image::{GrayImage, Luma};
use iroh::endpoint::presets;
use iroh::Endpoint;
use iroh_tickets::endpoint::EndpointTicket;
use irpc::Client;
use irpc_iroh::IrohRemoteConnection;
use qrcode::render::unicode::Dense1x2;
use qrcode::{Color, QrCode};
use smart_fan_proto::{
    GetLatest, GetStatus, SensorProtocol, SetThreshold, SetThresholdResponse, SENSOR_ALPN,
    THRESHOLD_MAX, THRESHOLD_MIN,
};

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
    /// Fetch the full device status: reading + fan state + threshold.
    Status {
        /// Endpoint ticket printed by the firmware.
        ticket: String,
    },
    /// Set the fan temperature threshold (requires the device's FAN_API_SECRET).
    SetThreshold {
        /// Endpoint ticket printed by the firmware.
        ticket: String,
        /// New temperature setpoint in °C.
        threshold: f32,
        /// The device's FAN_API_SECRET (from its serial log); reads $FAN_API_SECRET if unset.
        #[arg(long, env = "FAN_API_SECRET")]
        secret: String,
    },
    /// Round-trip a message off the device (echo protocol).
    Echo {
        /// Endpoint ticket printed by the firmware.
        ticket: String,
    },
    /// Write a QR-code PNG for the control URL (`base?ticket=<ticket>`). Scan it with
    /// a phone to open the GUI already connected to this device. Offline — no network.
    Qr {
        /// Endpoint ticket to embed. Use the short ticket — it's stable across reboots.
        ticket: String,
        /// Base URL of the hosted GUI.
        #[arg(
            long,
            default_value = "https://iroh.computer/blog/an-iroh-powered-smart-fan/smart-fan"
        )]
        base: String,
        /// Embed the FAN_API_SECRET so scanning unlocks control. Omit for a view-only
        /// QR. Treat a QR carrying the secret like a key — whoever scans it can control
        /// the fan.
        #[arg(long)]
        secret: Option<String>,
        /// Output PNG path.
        #[arg(long, default_value = "qr.png")]
        out: String,
    },
}

/// Render `base#ticket=<ticket>[&secret=<secret>]` as a QR-code PNG. Pure/offline —
/// no iroh, no network.
fn write_qr(ticket: &str, base: &str, secret: Option<&str>, out: &str) -> anyhow::Result<()> {
    // Ticket (and optional secret) ride in the URL fragment: it never reaches the
    // server / access logs — essential for the secret — and survives a static host's
    // trailing-slash redirect.
    let mut url = format!("{base}#ticket={ticket}");
    if let Some(s) = secret {
        url.push_str("&secret=");
        url.push_str(s);
    }

    let code = QrCode::new(url.as_bytes())?;
    let width = code.width();
    let colors = code.to_colors();

    // `scale` px per module, `quiet` modules of white margin (needed for scanners).
    let scale: u32 = 8;
    let quiet: u32 = 4;
    let dim = (width as u32 + 2 * quiet) * scale;
    let mut img = GrayImage::from_pixel(dim, dim, Luma([255]));
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == Color::Dark {
                let (px, py) = ((x as u32 + quiet) * scale, (y as u32 + quiet) * scale);
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(px + dx, py + dy, Luma([0]));
                    }
                }
            }
        }
    }
    img.save(out)?;

    // Also render the QR to the terminal so you can scan it directly to test — colors
    // inverted so it reads on a dark background. And print the encoded URL.
    let terminal = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    println!("{terminal}");
    println!("URL:   {url}");
    println!("Wrote: {out}");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // QR generation is offline — handle it before touching crypto / the network.
    if let Command::Qr { ticket, base, secret, out } = &cli.command {
        return write_qr(ticket, base, secret.as_deref(), out);
    }

    // iroh's TLS uses `ring` on the desktop; install its provider before we build.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install ring crypto provider");

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
        Command::Status { ticket } => {
            let addr: iroh::EndpointAddr = ticket.parse::<EndpointTicket>()?.into();
            println!("Connecting to {}…", addr.id);
            let conn = endpoint.connect(addr, SENSOR_ALPN).await?;
            let client: Client<SensorProtocol> = Client::boxed(IrohRemoteConnection::new(conn));
            match client.rpc(GetStatus).await? {
                Some(s) => println!(
                    "{:.1}°C  {:.1}%  fan {}  (threshold {:.0}°C)",
                    s.reading.temperature,
                    s.reading.humidity,
                    if s.fan { "on" } else { "off" },
                    s.threshold
                ),
                None => println!("No reading yet — the sensor hasn't produced one."),
            }
        }
        Command::SetThreshold { ticket, threshold, secret } => {
            let addr: iroh::EndpointAddr = ticket.parse::<EndpointTicket>()?.into();
            println!("Connecting to {}…", addr.id);
            let conn = endpoint.connect(addr, SENSOR_ALPN).await?;
            let client: Client<SensorProtocol> = Client::boxed(IrohRemoteConnection::new(conn));
            match client.rpc(SetThreshold { secret, threshold }).await? {
                SetThresholdResponse::Ok => println!("Threshold set to {threshold:.0}°C"),
                SetThresholdResponse::Unauthorized => println!("Rejected — wrong FAN_API_SECRET."),
                SetThresholdResponse::OutOfRange => println!(
                    "Rejected — threshold must be between {THRESHOLD_MIN:.0} and {THRESHOLD_MAX:.0}°C."
                ),
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
        Command::Qr { .. } => unreachable!("handled before endpoint setup"),
    }

    endpoint.close().await;
    Ok(())
}
