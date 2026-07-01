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
/// temperature setpoint (°C) the control loop is using. Everything a client needs in
/// one call. Kept separate from the `GetLatest` wire format so older clients that only
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

/// Set the fan temperature setpoint (°C). Protected: `secret` must match the device's
/// `FAN_API_SECRET` (printed on its serial log at startup), otherwise the device
/// rejects the call. The secret rides inside the encrypted QUIC connection, so it's
/// not exposed on the wire — but it's a bearer token, so keep it out of URLs and logs.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetThreshold {
    pub secret: String,
    pub threshold: f32,
}

/// Allowed range for the fan temperature setpoint (°C), inclusive. Shared so the
/// client can report the bounds and, if it wants, pre-validate before calling.
pub const THRESHOLD_MIN: f32 = 10.0;
pub const THRESHOLD_MAX: f32 = 50.0;

/// Outcome of a [`SetThreshold`] call — distinguishes why a set was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetThresholdResponse {
    /// Threshold updated.
    Ok,
    /// The secret didn't match `FAN_API_SECRET`.
    Unauthorized,
    /// The requested value was outside `THRESHOLD_MIN..=THRESHOLD_MAX`.
    OutOfRange,
}

/// The sensor RPC service. `rpc_requests` generates the [`SensorMessage`] enum
/// (the channel-carrying form) consumed by the server handler.
///
/// Variants are append-only: each new method goes at the end so existing
/// discriminants are undisturbed and older clients keep working.
#[rpc_requests(message = SensorMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum SensorProtocol {
    #[rpc(tx = oneshot::Sender<Option<Reading>>)]
    GetLatest(GetLatest),
    #[rpc(tx = oneshot::Sender<Option<Status>>)]
    GetStatus(GetStatus),
    #[rpc(tx = oneshot::Sender<SetThresholdResponse>)]
    SetThreshold(SetThreshold),
}
