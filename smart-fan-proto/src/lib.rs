//! Shared irpc protocol for the smart-fan ESP32.
//!
//! Defines the wire types and the irpc service surface used by both the firmware
//! (server) and the CLI (client). Deliberately board-agnostic: no esp-idf deps and
//! no `[patch]`, so it builds on the host as well as on `xtensa-esp32-espidf`.

use irpc::channel::oneshot;
use irpc::rpc_requests;
use serde::{Deserialize, Serialize};

/// The ALPN for the smart-fan sensor RPC protocol.
pub const SENSOR_ALPN: &[u8] = b"smart-fan/sensor/0";

/// A single sensor reading: temperature in °C, relative humidity in %.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Reading {
    pub temperature: f32,
    pub humidity: f32,
}

/// Full device status: the latest [`Reading`] plus the fan actuator state and the
/// humidity setpoint (%) the control loop is using. Everything a client needs in one
/// call. Kept separate from the `GetLatest` wire format so older clients that only
/// speak `GetLatest` keep working unchanged.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Status {
    pub reading: Reading,
    pub fan: bool,
    pub threshold: f32,
}

/// Request the most recent reading. Returns `None` until the first successful read.
#[derive(Debug, Serialize, Deserialize)]
pub struct GetLatest;

/// Request the full device status. Returns `None` until the first successful read.
#[derive(Debug, Serialize, Deserialize)]
pub struct GetStatus;

/// The sensor RPC service. `rpc_requests` generates the [`SensorMessage`] enum
/// (the channel-carrying form) consumed by the server handler.
///
/// Variants are append-only: `GetStatus` was added after `GetLatest`, so its
/// discriminant doesn't disturb the existing one.
#[rpc_requests(message = SensorMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum SensorProtocol {
    #[rpc(tx = oneshot::Sender<Option<Reading>>)]
    GetLatest(GetLatest),
    #[rpc(tx = oneshot::Sender<Option<Status>>)]
    GetStatus(GetStatus),
}
