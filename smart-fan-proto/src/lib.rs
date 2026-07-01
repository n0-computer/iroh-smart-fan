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

/// Request the most recent reading. Returns `None` until the first successful read.
#[derive(Debug, Serialize, Deserialize)]
pub struct GetLatest;

/// The sensor RPC service. `rpc_requests` generates the [`SensorMessage`] enum
/// (the channel-carrying form) consumed by the server handler.
#[rpc_requests(message = SensorMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum SensorProtocol {
    #[rpc(tx = oneshot::Sender<Option<Reading>>)]
    GetLatest(GetLatest),
}
