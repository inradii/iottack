# noise handshake

Connects to an ESPHome native API endpoint, sends a CLIENT_HELLO, and parses
the SERVER_HELLO frame that the device returns before the Noise NNpsk0
handshake completes. Also probes whether the unencrypted plaintext protocol
variant is accepted.

## Checks performed

| Check | Finding | Severity |
| ----- | ------- | -------- |
| Port reachable, valid SERVER_HELLO received | ESPHome Noise API detected | Info |
| Node name or MAC address in SERVER_HELLO | Pre-authentication information disclosure | Medium |
| Device responds to plaintext CLIENT_HELLO (indicator `0x00`) | Plaintext protocol accepted | High |
| TCP connection refused or timeout | Port not reachable | Info |
| Malformed or missing SERVER_HELLO | Unexpected response format | Info |

## Background

ESPHome's SERVER_HELLO frame is transmitted **before** the Noise NNpsk0
handshake and before the pre-shared key is verified. It contains:

- Protocol version (1 byte)
- Node name as a null-terminated string
- MAC address as a null-terminated string

This information is visible to any host that can reach port 6053, regardless
of whether the host possesses the PSK.

## Usage

```sh
iottack noise handshake --host <HOST> [OPTIONS]
```

### Options

| Flag | Short | Default | Description |
| ---- | ----- | ------- | ----------- |
| `--host <HOST>` | `-H` | *(required)* | Target hostname or IP address |
| `--port <PORT>` | `-p` | `6053` | Native API TCP port |
| `--timeout <MS>` | `-t` | `5000` | Connect and read timeout (ms) |
| `--output <FILE>` | `-o` | - | Save result to a file |
| `--format <FORMAT>` | `-f` | `text` | `text` or `json` |
| `--verbose` | `-v` | - | Increase log verbosity |

## Examples

Probe a device on the default port:

```sh
$ iottack noise handshake --host 192.168.1.50
[INFO]  ESPHome Noise API detected
        Device at 192.168.1.50:6053 responded to CLIENT_HELLO with a valid
        SERVER_HELLO frame (protocol version 1).
        node_name="living-room-sensor"  mac_address="AA:BB:CC:DD:EE:FF"  protocol_version=1

[MEDIUM] Pre-authentication information disclosure
         The ESPHome SERVER_HELLO frame transmits the device node name and
         MAC address in plaintext before the Noise NNpsk0 handshake completes.
         node_name="living-room-sensor"  mac_address="AA:BB:CC:DD:EE:FF"
```

Probe a device using a non-standard port and save results as JSON:

```sh
$ iottack noise handshake --host 192.168.1.50 --port 6054 --format json --output result.json
```

Example output when plaintext is accepted:

```sh
[HIGH]  Plaintext protocol accepted
        The device responded to a plaintext (unencrypted) CLIENT_HELLO
        (indicator byte 0x00).
        indicator=0x00 response received
```
