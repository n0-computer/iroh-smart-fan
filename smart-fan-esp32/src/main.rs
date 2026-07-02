use core::convert::TryInto;

use esp_idf_svc::eventloop::{EspSubscription, EspSystemEventLoop, System};
use esp_idf_svc::hal::gpio::*;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi, WifiEvent};
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::SecretKey;
use iroh::tls::CaTlsConfig;
use iroh_tickets::endpoint::EndpointTicket;
use irpc::WithChannels;
use irpc_iroh::read_request;
use log::{info, warn};
use smart_fan_proto::{
    Reading, SensorMessage, SensorProtocol, SetThreshold, SetThresholdResponse, Status,
    SENSOR_ALPN, THRESHOLD_MAX, THRESHOLD_MIN,
};
use std::sync::{mpsc, Arc, Mutex};

mod std_dns_resolver;
mod quic_crypto_provider;
mod insecure_verifier;

/// The ALPN for the echo protocol.
const ECHO_ALPN: &[u8] = b"echo/0";

/// Optional build-time override for the node's secret key. Normally unset: the device
/// generates a key on first boot and persists it in NVS (see [`resolve_iroh_secret`]),
/// so the endpoint ID is stable across reboots *and* app reflashes, and unique per
/// device. Set IROH_SECRET=<64 hex chars or base32> only to pin a *specific* identity.
const IROH_SECRET: Option<&str> = option_env!("IROH_SECRET");

/// Optional build-time override for the fan-control API secret. Normally unset: the
/// device generates one on first boot and persists it in NVS (see [`resolve_fan_secret`]).
const FAN_API_SECRET: Option<&str> = option_env!("FAN_API_SECRET");

/// NVS namespace holding the device-generated secrets.
const NVS_NAMESPACE: &str = "smartfan";

const WIFI_CONFIG: &str = match option_env!("WIFI_CONFIG") {
    Some(value) => value,
    None => panic!("WIFI_CONFIG is not set. Build with WIFI_CONFIG='SSID:PASSWORD' cargo build"),
};

/// Fill `buf` with hardware random bytes. Call after WiFi/RF is up so the RNG has real
/// entropy — before that, esp_fill_random is only pseudo-random.
fn fill_random(buf: &mut [u8]) {
    unsafe {
        esp_idf_svc::sys::esp_fill_random(buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len())
    };
}

/// Resolve the node's iroh secret key. An explicit build-time IROH_SECRET wins;
/// otherwise a device-local key is read from NVS, generating + persisting one on first
/// boot. NVS survives app reflashes (only `espflash erase-flash` clears it), so the
/// endpoint ID stays stable across firmware updates and is unique per device.
fn resolve_iroh_secret(nvs: &mut EspNvs<NvsDefault>) -> SecretKey {
    if let Some(hex) = IROH_SECRET {
        return hex
            .parse()
            .expect("IROH_SECRET must be valid hex (64 chars) or base32");
    }
    let mut key = [0u8; 32];
    if let Ok(Some(_)) = nvs.get_blob("iroh_secret", &mut key) {
        return SecretKey::from_bytes(&key);
    }
    fill_random(&mut key);
    nvs.set_blob("iroh_secret", &key)
        .expect("persist iroh_secret to NVS");
    info!("Generated a new node identity and stored it in NVS");
    SecretKey::from_bytes(&key)
}

/// Resolve the fan-control API secret (16 hex chars). An explicit FAN_API_SECRET wins;
/// otherwise it's device-generated + persisted in NVS on first boot.
fn resolve_fan_secret(nvs: &mut EspNvs<NvsDefault>) -> Arc<str> {
    if let Some(s) = FAN_API_SECRET {
        return s.into();
    }
    // Stored as raw 8 bytes (like the iroh key); rendered to the 16-char hex string
    // used on the wire / in the GUI only here at the point of use.
    let mut bytes = [0u8; 8];
    if !matches!(nvs.get_blob("fan_api_secret", &mut bytes), Ok(Some(_))) {
        fill_random(&mut bytes);
        nvs.set_blob("fan_api_secret", &bytes)
            .expect("persist fan_api_secret to NVS");
        info!("Generated a new fan API secret and stored it in NVS");
    }
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into()
}

// ESP-IDF doesn't provide gethostname, but resolv_conf (via hickory-resolver) references it.
#[no_mangle]
unsafe extern "C" fn gethostname(name: *mut core::ffi::c_char, len: usize) -> core::ffi::c_int {
    if len > 0 && !name.is_null() {
        unsafe {
            *name = 0;
        }
    }
    0
}

// --- DHT22 sensor (single-wire bit-bang, wired on GPIO26) --------------------

// `Reading` now comes from smart_fan_proto (shared with the CLI).

/// Microseconds since boot (ESP-IDF hardware timer).
fn micros() -> i64 {
    unsafe { esp_idf_svc::sys::esp_timer_get_time() }
}

/// Busy-wait `us` microseconds via the hardware-calibrated ROM delay.
fn busy_wait_us(us: u32) {
    unsafe { esp_idf_svc::sys::esp_rom_delay_us(us) }
}

/// Per-edge timeout for the DHT22 bit-bang. Generous on purpose: long cables slow
/// the pull-up's rise, and the bit-bang can be briefly preempted by WiFi/QUIC.
const DHT_EDGE_TIMEOUT_US: i64 = 3_000;

/// Busy-wait until the pin reaches `level`, or `timeout_us` elapses.
fn wait_for(
    pin: &PinDriver<'_, impl IOPin, InputOutput>,
    level: Level,
    timeout_us: i64,
) -> Result<(), &'static str> {
    let start = micros();
    while pin.get_level() != level {
        if micros() - start > timeout_us {
            return Err("timeout");
        }
    }
    Ok(())
}

/// Read 40 bits from a DHT22. The pin must be open-drain input/output with a pull-up.
fn read_dht22(pin: &mut PinDriver<'_, impl IOPin, InputOutput>) -> Result<Reading, &'static str> {
    // Start signal: pull low ≥1 ms, then release (pull-up brings it high).
    pin.set_low().map_err(|_| "set_low")?;
    busy_wait_us(3_000);
    pin.set_high().map_err(|_| "set_high")?;
    busy_wait_us(40);

    // Response: sensor pulls low ~80 µs, then high ~80 µs.
    wait_for(pin, Level::Low, DHT_EDGE_TIMEOUT_US)?;
    wait_for(pin, Level::High, DHT_EDGE_TIMEOUT_US)?;
    wait_for(pin, Level::Low, DHT_EDGE_TIMEOUT_US)?;

    // 40 data bits: each starts ~50 µs low, then a variable-length high.
    let mut data = [0u8; 5];
    for i in 0..40 {
        wait_for(pin, Level::High, DHT_EDGE_TIMEOUT_US)?;
        let t = micros();
        wait_for(pin, Level::Low, DHT_EDGE_TIMEOUT_US)?;
        // 26-28 µs high → 0, ~70 µs high → 1.
        if micros() - t > 40 {
            data[i / 8] |= 1 << (7 - (i % 8));
        }
    }

    // Checksum: sum of the first 4 bytes, truncated to u8.
    let sum: u8 = data[..4].iter().map(|&b| b as u16).sum::<u16>() as u8;
    if sum != data[4] {
        return Err("checksum mismatch");
    }

    let humidity = ((data[0] as u16) << 8 | data[1] as u16) as f32 / 10.0;
    let raw = ((data[2] as u16 & 0x7F) << 8) | data[3] as u16;
    let temperature = if data[2] & 0x80 != 0 { -(raw as f32) } else { raw as f32 } / 10.0;

    Ok(Reading {
        temperature,
        humidity,
    })
}

/// Read a DHT22, retrying once on failure. Most failures are transient — the
/// bit-bang frame gets preempted by WiFi/QUIC and an edge is missed — so an
/// immediate re-read recovers the bulk of them.
fn read_dht22_retry(pin: &mut PinDriver<'_, impl IOPin, InputOutput>) -> Result<Reading, &'static str> {
    match read_dht22(pin) {
        Ok(reading) => Ok(reading),
        Err(_) => {
            busy_wait_us(2_000); // let the line settle before a fresh start pulse
            read_dht22(pin)
        }
    }
}

/// Default temperature (°C) at/above which the fan turns on. Stored in [`State`]
/// rather than as a plain const so a later step can make it settable from the GUI.
const DEFAULT_TEMP_THRESHOLD: f32 = 25.0;
/// Hysteresis band (°C): once on, the fan only turns off after the temperature drops
/// this far below the threshold, so it doesn't chatter around the setpoint.
const FAN_HYSTERESIS: f32 = 1.0;

/// Device state behind a single lock, shared between the sensor thread (writer) and
/// the RPC handler (reader): the latest reading, the fan on/off state, and the fan
/// temperature setpoint (mutable so a later step can set it from the GUI). The RPC
/// handlers project out the fields each call needs.
#[derive(Debug, Clone, Copy)]
struct State {
    reading: Option<Reading>,
    fan: bool,
    threshold: f32,
}

/// Sensor + actuator thread: read the DHT22 on GPIO26 every 2 s, drive the fan on
/// GPIO25 from the temperature vs. the threshold with hysteresis, and publish the
/// reading + fan state into the shared `state` for the RPC handler to serve.
///
/// `config_rx` lets a `SetThreshold` RPC wake this thread so the fan reacts to a new
/// setpoint at once (re-applying against the last reading) instead of waiting for the
/// next 2 s tick — without reading the DHT22 any more often.
fn run_sensor(
    pin: Gpio26,
    fan_pin: Gpio25,
    state: Arc<Mutex<State>>,
    config_rx: mpsc::Receiver<()>,
) {
    let mut sensor =
        PinDriver::input_output_od(pin).expect("Failed to configure GPIO26 (DHT22)");
    sensor.set_pull(Pull::Up).expect("pull-up");
    sensor.set_high().expect("high");

    // Fan output on GPIO25 (HIGH = on). Wire an LED + ~220–330 Ω to GND for now.
    let mut fan = PinDriver::output(fan_pin).expect("Failed to configure GPIO25 (fan)");
    fan.set_low().expect("fan low");
    let mut fan_on = false;
    let mut last: Option<Reading> = None;

    // DHT22 needs ≥1 s after power-on before the first read.
    std::thread::sleep(std::time::Duration::from_secs(2));

    loop {
        // Fresh sample once per outer iteration (a config wake-up re-applies against
        // `last` without a new read).
        match read_dht22_retry(&mut sensor) {
            Ok(r) => last = Some(r),
            Err(e) => warn!("DHT22 read failed: {e}"),
        }

        loop {
            // Apply the fan control from the latest reading + current threshold.
            if let Some(r) = last {
                let threshold = {
                    let mut s = state.lock().expect("poisoned");
                    // Hysteresis: once on, stay on until the temperature drops a band
                    // below the setpoint; once off, turn on only at the setpoint.
                    fan_on = if fan_on {
                        r.temperature >= s.threshold - FAN_HYSTERESIS
                    } else {
                        r.temperature >= s.threshold
                    };
                    s.reading = Some(r);
                    s.fan = fan_on;
                    s.threshold
                };
                // Drive the GPIO and log outside the lock.
                let _ = if fan_on { fan.set_high() } else { fan.set_low() };
                info!(
                    "DHT22: {:.1}°C  {:.1}%  fan={}  (threshold {:.0}°C)",
                    r.temperature,
                    r.humidity,
                    if fan_on { "on" } else { "off" },
                    threshold,
                );
            }

            // Wait for the next tick, but wake immediately on a config change and
            // re-apply against `last`. On timeout, break out to take a fresh reading.
            match config_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(()) => {
                    while config_rx.try_recv().is_ok() {} // coalesce a burst of changes
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // Server gone (shouldn't happen) — fall back to a plain interval.
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    break;
                }
            }
        }
    }
}

/// Connect to WiFi and install an auto-reconnect handler. esp-idf does not re-associate
/// on its own after the AP drops, so we subscribe to `WifiEvent` and call
/// `esp_wifi_connect()` on every disconnect (a failed reconnect just fires another
/// disconnect, so this retries until the AP is back). Returns the subscription, which
/// must be kept alive for the handler to keep firing.
fn connect_wifi(
    modem: Modem,
    nvs: EspDefaultNvsPartition,
) -> (
    BlockingWifi<EspWifi<'static>>,
    EspSubscription<'static, System>,
    std::net::Ipv4Addr,
) {
    let (ssid, password) = WIFI_CONFIG
        .split_once(':')
        .expect("WIFI_CONFIG must be in the format SSID:PASSWORD");

    info!("Connecting to WiFi network: {ssid}");

    let sys_loop = EspSystemEventLoop::take().expect("Failed to take event loop");

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs))
            .expect("Failed to create EspWifi"),
        sys_loop.clone(),
    )
    .expect("Failed to create BlockingWifi");

    let config = Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().expect("SSID too long"),
        password: password.try_into().expect("Password too long"),
        ..Default::default()
    });

    wifi.set_configuration(&config)
        .expect("Failed to set WiFi configuration");
    wifi.start().expect("Failed to start WiFi");
    info!("WiFi started");

    wifi.connect().expect("Failed to connect to WiFi");
    info!("WiFi connected");

    wifi.wait_netif_up().expect("Failed to wait for netif up");
    let ip_info = wifi
        .wifi()
        .sta_netif()
        .get_ip_info()
        .expect("Failed to get IP info");
    info!("WiFi DHCP info: {ip_info:?}");

    let ip = ip_info.ip;

    // Auto-reconnect: re-associate whenever the STA disconnects (e.g. AP beacon
    // timeout). Keep the returned subscription alive or the handler stops firing.
    let subscription = sys_loop
        .subscribe::<WifiEvent, _>(|event| {
            if matches!(event, WifiEvent::StaDisconnected(_)) {
                warn!("WiFi disconnected — reconnecting");
                unsafe {
                    esp_idf_svc::sys::esp_wifi_connect();
                }
            }
        })
        .expect("Failed to subscribe to WiFi events");

    (
        wifi,
        subscription,
        std::net::Ipv4Addr::new(
            ip.octets()[0],
            ip.octets()[1],
            ip.octets()[2],
            ip.octets()[3],
        ),
    )
}

fn sync_time_sntp() -> esp_idf_svc::sntp::EspSntp<'static> {
    info!("Starting SNTP time sync...");
    let sntp = esp_idf_svc::sntp::EspSntp::new_default().expect("Failed to start SNTP");
    let mut retries = 0;
    while sntp.get_sync_status() != esp_idf_svc::sntp::SyncStatus::Completed {
        retries += 1;
        if retries > 30 {
            warn!("SNTP sync timed out after 30s, continuing anyway");
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    if sntp.get_sync_status() == esp_idf_svc::sntp::SyncStatus::Completed {
        info!("SNTP synced");
    }
    sntp
}

/// Echo protocol handler — the simple bytes-in/bytes-out side protocol.
#[derive(Debug, Clone)]
struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = connection.accept_bi().await?;
        let bytes = tokio::io::copy(&mut recv, &mut send).await?;
        info!("echoed {bytes} byte(s)");
        send.finish()?;
        connection.closed().await;
        Ok(())
    }
}

/// iroh `ProtocolHandler` for the sensor RPC. Cloneable shared-state server: every
/// accepted connection reads requests and answers them from the latest reading.
#[derive(Debug, Clone)]
struct SensorServer {
    state: Arc<Mutex<State>>,
    /// Wakes the sensor thread to apply a new threshold immediately.
    config_tx: mpsc::Sender<()>,
    /// Shared secret required to authorize `SetThreshold` (device-generated, from NVS).
    fan_secret: Arc<str>,
}

impl ProtocolHandler for SensorServer {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        while let Some(msg) = read_request::<SensorProtocol>(&conn).await? {
            match msg {
                SensorMessage::GetLatest(msg) => {
                    let WithChannels { tx, .. } = msg;
                    // Original API: project out just the reading.
                    let reading = self.state.lock().expect("poisoned").reading;
                    tx.send(reading).await.ok();
                }
                SensorMessage::GetStatus(msg) => {
                    let WithChannels { tx, .. } = msg;
                    let s = *self.state.lock().expect("poisoned");
                    let status = s.reading.map(|reading| Status {
                        reading,
                        fan: s.fan,
                        threshold: s.threshold,
                    });
                    tx.send(status).await.ok();
                }
                SensorMessage::SetThreshold(msg) => {
                    let WithChannels { inner, tx, .. } = msg;
                    let SetThreshold { secret, threshold } = inner;
                    let response = if secret != *self.fan_secret {
                        warn!("rejected SetThreshold: bad API secret");
                        SetThresholdResponse::Unauthorized
                    } else if !(THRESHOLD_MIN..=THRESHOLD_MAX).contains(&threshold) {
                        warn!("rejected SetThreshold: {threshold:.0}°C out of range");
                        SetThresholdResponse::OutOfRange
                    } else {
                        self.state.lock().expect("poisoned").threshold = threshold;
                        // Wake the sensor thread so the fan reacts to the new setpoint
                        // right away instead of on the next read tick.
                        let _ = self.config_tx.send(());
                        info!("threshold set to {threshold:.0}°C via API");
                        SetThresholdResponse::Ok
                    };
                    tx.send(response).await.ok();
                }
            }
        }
        conn.closed().await;
        Ok(())
    }
}

/// Our own logger instance (rather than `EspLogger::initialize_default`) so we can
/// set per-target log levels — see the note in `main`.
static LOGGER: esp_idf_svc::log::EspLogger = esp_idf_svc::log::EspLogger::new();

fn main() {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to ESP-IDF logging via our own logger instance so we can
    // silence noisy targets. (esp_log_level_get caches per-tag levels by the CString
    // *address*, so the level must be set through the same EspLogger that emits the
    // records — not initialize_default's private global, nor a raw esp_log_level_set.)
    log::set_logger(&LOGGER).expect("set logger");
    LOGGER.initialize();
    // iroh logs every datagram at INFO from this target (`poll_send; …`) — pure serial
    // spam. Drop just this target to WARN; every other target keeps its level.
    LOGGER
        .set_target_level("iroh::socket::transports", log::LevelFilter::Warn)
        .ok();
    // tracing's `log` feature emits span lifecycle events (`tx`, `QADv4`, …) under this
    // target, some at WARN — all noise on serial. Silence the target entirely.
    LOGGER
        .set_target_level("tracing::span", log::LevelFilter::Off)
        .ok();

    // Register eventfd VFS — needed by mio's poll implementation which powers tokio I/O
    let eventfd_config = esp_idf_svc::sys::esp_vfs_eventfd_config_t {
        max_fds: 5,
        ..Default::default()
    };
    unsafe { esp_idf_svc::sys::esp_vfs_eventfd_register(&eventfd_config) };

    // Pure-Rust crypto provider with minimal QUIC support
    let provider = std::sync::Arc::new(quic_crypto_provider::provider());

    // Split peripherals once: the modem drives WiFi, GPIO26 the DHT22, GPIO25 the fan.
    let peripherals = Peripherals::take().expect("Failed to take peripherals");
    let modem = peripherals.modem;
    let sensor_pin = peripherals.pins.gpio26;
    let fan_pin = peripherals.pins.gpio25;

    // Single shared state behind one lock, written by the sensor thread and read by
    // the RPC handler.
    let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State {
        reading: None,
        fan: false,
        threshold: DEFAULT_TEMP_THRESHOLD,
    }));

    // Wake channel: a SetThreshold RPC signals the sensor thread to apply the new
    // setpoint immediately instead of on the next read tick.
    let (config_tx, config_rx) = mpsc::channel::<()>();

    // Read the DHT22 + drive the fan on their own thread.
    let sensor_state = state.clone();
    std::thread::Builder::new()
        .name("sensor".into())
        .stack_size(8192)
        .spawn(move || run_sensor(sensor_pin, fan_pin, sensor_state, config_rx))
        .expect("Failed to spawn sensor thread");

    // One NVS partition, shared: WiFi stores calibration in it, and we keep our own
    // namespace for the device-generated secrets.
    let nvs = EspDefaultNvsPartition::take().expect("Failed to take NVS partition");
    let (_wifi, _wifi_reconnect, wifi_ip) = connect_wifi(modem, nvs.clone());

    // Sync system clock via SNTP — needed for TLS certificate validation
    // Keep _sntp alive so the periodic re-sync continues
    let _sntp = sync_time_sntp();

    // IMPORTANT: keep this AFTER connect_wifi. First-boot secret generation draws on
    // esp_fill_random, which is only cryptographically secure while an RF subsystem
    // (WiFi/BT) is active — running it before WiFi is up would seed the device identity
    // and API secret from weak pseudo-randomness. (Reads on subsequent boots don't
    // care, but don't reorder this.) Each secret is generated + persisted in NVS on
    // first boot, then stable across reboots and app reflashes.
    let (iroh_secret, fan_secret) = {
        let mut nvs = EspNvs::new(nvs, NVS_NAMESPACE, true).expect("open NVS namespace");
        (resolve_iroh_secret(&mut nvs), resolve_fan_secret(&mut nvs))
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .thread_stack_size(4096)
        .build()
        .expect("Failed to create tokio runtime");

    rt.block_on(async {
        let dns_resolver = iroh::dns::DnsResolver::custom(std_dns_resolver::StdDnsResolver);

        let mut builder = iroh::Endpoint::builder(iroh::endpoint::presets::Empty)
            .crypto_provider(provider)
            .ca_tls_config(CaTlsConfig::custom_server_cert_verifier(
                insecure_verifier::skip_verify(),
            ))
            .dns_resolver(dns_resolver)
            .relay_mode(iroh::RelayMode::Default)
            .address_lookup(iroh::address_lookup::PkarrPublisher::n0_dns())
            .address_lookup(iroh::address_lookup::PkarrResolver::n0_dns())
            // Disable HTTPS latency probes and captive-portal detection: both make
            // real-cert TLS connections, which our minimal crypto provider (no RSA,
            // AES-128-GCM + X25519 only) cannot verify. QAD (UDP) probes still
            // measure relay latency.
            .net_report_config({
                let mut c = iroh::NetReportConfig::default();
                c.https_probes = false;
                c.captive_portal_check = false;
                c
            });

        builder = builder.secret_key(iroh_secret);

        let endpoint = builder.bind().await.expect("unable to bind endpoint");

        let endpoint_id = endpoint.addr().id;
        let port = endpoint
            .bound_sockets()
            .first()
            .map(|s| s.port())
            .expect("no bound socket");

        // Short ticket: just the endpoint ID (no addresses)
        let short_ticket = EndpointTicket::new(iroh::EndpointAddr::new(endpoint_id));

        // Long ticket: includes WiFi IP + bound port
        let mut addr_with_ip = endpoint.addr();
        addr_with_ip
            .addrs
            .insert(iroh::TransportAddr::Ip(std::net::SocketAddr::new(
                wifi_ip.into(),
                port,
            )));
        let long_ticket = EndpointTicket::new(addr_with_ip);

        let _router = Router::builder(endpoint)
            .accept(ECHO_ALPN, Echo)
            .accept(
                SENSOR_ALPN,
                SensorServer {
                    state,
                    config_tx,
                    fan_secret: fan_secret.clone(),
                },
            )
            .spawn();

        info!("Iroh endpoint bound");
        info!("  Listening on: {wifi_ip}:{port}");
        info!("  Endpoint ID: {endpoint_id}");
        info!("  Short ticket: {short_ticket}");
        info!("  Long ticket:  {long_ticket}");
        info!("  Fan API secret: {fan_secret}");
        info!("  Sensor ALPN:  {}", String::from_utf8_lossy(SENSOR_ALPN));

        info!("Router started, accepting connections");

        // Keep the router running indefinitely
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    });
}
