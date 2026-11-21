use crate::core::{
    CliArg, Plugin, PluginConfig, PluginFinding, PluginMeta, PluginResult, PluginStatus, Severity,
};
use crate::plugin_meta;
use anyhow::Result;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_PORT: u16 = 6053;
const DEFAULT_TIMEOUT_MS: u64 = 5000;

const INDICATOR_NOISE: u8 = 0x01;
const INDICATOR_PLAINTEXT: u8 = 0x00;

struct ServerHello {
    protocol_version: u8,
    node_name: String,
    mac_address: String,
}

/// Send a CLIENT_HELLO frame with the given indicator byte.
///
/// Frame layout: [indicator(1)] [size_hi(1)] [size_lo(1)] [protocol_version(1)]
fn send_client_hello(stream: &mut TcpStream, indicator: u8) -> Result<()> {
    stream.write_all(&[indicator, 0x00, 0x01, 0x01])?;
    Ok(())
}

/// Parse the SERVER_HELLO frame returned by an ESPHome Noise device.
///
/// Frame layout:
///   [0x01] [size_hi] [size_lo] [protocol_version] [node_name\0] [mac_addr\0]
///
/// The node name and MAC address are transmitted in plaintext before the
/// NNpsk0 handshake completes.
fn read_server_hello(stream: &mut TcpStream) -> Result<Option<ServerHello>> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header)?;

    if header[0] != INDICATOR_NOISE {
        return Ok(None);
    }

    let payload_len = u16::from_be_bytes([header[1], header[2]]) as usize;
    if payload_len == 0 {
        return Ok(None);
    }

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload)?;

    let protocol_version = payload[0];

    // Extract up to two null-terminated strings from the remainder of the frame.
    let mut strings: Vec<String> = Vec::new();
    let mut start = 1;
    for i in 1..payload.len() {
        if payload[i] == 0 {
            let s = String::from_utf8_lossy(&payload[start..i]).to_string();
            strings.push(s);
            start = i + 1;
            if strings.len() == 2 {
                break;
            }
        }
    }
    while strings.len() < 2 {
        strings.push(String::new());
    }

    Ok(Some(ServerHello {
        protocol_version,
        node_name: strings[0].clone(),
        mac_address: strings[1].clone(),
    }))
}

/// Attempt a plaintext CLIENT_HELLO and return true if the device responds.
///
/// Plaintext frame: [0x00][payload_size VarInt][msg_type VarInt]
/// A minimal HelloRequest (msg type 1, empty payload): [0x00][0x00][0x01]
fn probe_plaintext(addr: SocketAddr, timeout: Duration) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
        return false;
    };
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();

    if stream.write_all(&[INDICATOR_PLAINTEXT, 0x00, 0x01]).is_err() {
        return false;
    }

    let mut buf = [0u8; 16];
    matches!(stream.read(&mut buf), Ok(n) if n > 0)
}

/// Handshake plugin for the ESPHome Noise protocol.
pub struct HandshakePlugin {
    host: String,
    port: u16,
    timeout_ms: u64,
}

/// Create a new instance of the handshake plugin with default settings.
impl HandshakePlugin {
    pub fn new() -> Self {
        Self {
            host: String::new(),
            port: DEFAULT_PORT,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

/// Default implementation for the handshake plugin.
impl Default for HandshakePlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// Implementation of the handshake plugin for the ESPHome Noise protocol.
impl Plugin for HandshakePlugin {
    fn meta(&self) -> PluginMeta {
        plugin_meta!(
            summary: "ESPHome Noise protocol handshake probe",
            description: "Connects to an ESPHome native API endpoint and inspects the \
                          pre-encryption SERVER_HELLO frame, which reveals the device node \
                          name and MAC address before the Noise NNpsk0 handshake completes. \
                          Also checks whether the unencrypted plaintext protocol variant \
                          is accepted by the target.",
            needs_root: false,
            args: &[
                CliArg {
                    name: "host",
                    short: Some('H'),
                    help: "Target host (IP or hostname)",
                    value_name: "HOST",
                    default: None,
                    is_flag: false,
                },
                CliArg {
                    name: "port",
                    short: Some('p'),
                    help: "Native API port",
                    value_name: "PORT",
                    default: Some("6053"),
                    is_flag: false,
                },
                CliArg {
                    name: "timeout",
                    short: Some('t'),
                    help: "Connection timeout in milliseconds",
                    value_name: "MS",
                    default: Some("5000"),
                    is_flag: false,
                },
            ],
        )
    }

    fn initialize(&mut self, config: &PluginConfig) -> Result<()> {
        self.host = config.require("host")?.to_string();
        self.port = config.parse_or("port", DEFAULT_PORT);
        self.timeout_ms = config.parse_or("timeout", DEFAULT_TIMEOUT_MS);
        Ok(())
    }

    fn execute(&mut self) -> Result<PluginResult> {
        let mut result = PluginResult::new(self.meta().name);
        let timeout = Duration::from_millis(self.timeout_ms);

        let addr_str = format!("{}:{}", self.host, self.port);
        let addr: SocketAddr = match addr_str.to_socket_addrs()?.next() {
            Some(a) => a,
            None => {
                result.add(PluginFinding::new(
                    Severity::Info,
                    "Could not resolve host",
                    format!("DNS resolution failed for {}", self.host),
                ));
                result.finish(PluginStatus::Fail);
                return Ok(result);
            }
        };

        // --- Noise handshake probe
        let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => s,
            Err(e) => {
                result.add(PluginFinding::new(
                    Severity::Info,
                    "Port not reachable",
                    format!(
                        "TCP connection to {}:{} failed: {}",
                        self.host, self.port, e
                    ),
                ));
                result.finish(PluginStatus::Fail);
                return Ok(result);
            }
        };
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;

        if let Err(e) = send_client_hello(&mut stream, INDICATOR_NOISE) {
            result.add(PluginFinding::new(
                Severity::Info,
                "Failed to send CLIENT_HELLO",
                format!("Write error: {e}"),
            ));
            result.finish(PluginStatus::Fail);
            return Ok(result);
        }

        match read_server_hello(&mut stream) {
            Ok(Some(hello)) => {
                result.add(
                    PluginFinding::new(
                        Severity::Info,
                        "ESPHome Noise API detected",
                        format!(
                            "Device at {}:{} responded to CLIENT_HELLO with a valid \
                             SERVER_HELLO frame (protocol version {}).",
                            self.host, self.port, hello.protocol_version
                        ),
                    )
                    .with_evidence(format!(
                        "node_name=\"{}\"  mac_address=\"{}\"  protocol_version={}",
                        hello.node_name, hello.mac_address, hello.protocol_version
                    )),
                );

                if !hello.node_name.is_empty() || !hello.mac_address.is_empty() {
                    result.add(
                        PluginFinding::new(
                            Severity::Medium,
                            "Pre-authentication information disclosure",
                            "The ESPHome SERVER_HELLO frame transmits the device node name \
                             and MAC address in plaintext before the Noise NNpsk0 handshake \
                             completes and before the pre-shared key is verified. Any host \
                             that can reach this port can read device identifiers without \
                             possessing the PSK.",
                        )
                        .with_evidence(format!(
                            "node_name=\"{}\"  mac_address=\"{}\"",
                            hello.node_name, hello.mac_address
                        )),
                    );
                }
            }
            Ok(None) => {
                result.add(PluginFinding::new(
                    Severity::Info,
                    "Unexpected response format",
                    "The host responded but the SERVER_HELLO frame could not be parsed. \
                     The target may not be an ESPHome device or may use a different \
                     protocol version.",
                ));
            }
            Err(e) => {
                result.add(PluginFinding::new(
                    Severity::Info,
                    "No SERVER_HELLO received",
                    format!(
                        "Read timed out or was reset after CLIENT_HELLO: {e}. The device \
                         may require a valid Noise ephemeral key before responding."
                    ),
                ));
            }
        }
        drop(stream);

        // --- Plaintext protocol probe
        if probe_plaintext(addr, timeout) {
            result.add(
                PluginFinding::new(
                    Severity::High,
                    "Plaintext protocol accepted",
                    "The device responded to a plaintext (unencrypted) CLIENT_HELLO \
                     (indicator byte 0x00). This indicates the native API may be operable \
                     without Noise encryption, exposing all API traffic, including \
                     authentication credentials, in cleartext.",
                )
                .with_evidence("indicator=0x00 response received"),
            );
        }

        result.finish(PluginStatus::Pass);
        Ok(result)
    }
}

inventory::submit! {
    crate::PluginFactory(|| Box::new(HandshakePlugin::new()) as Box<dyn crate::Plugin>)
}
