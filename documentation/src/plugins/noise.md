# Noise

The [Noise Protocol Framework](https://noiseprotocol.org/) is a family of
cryptographic handshake patterns built on Diffie-Hellman key exchange, symmetric
encryption, and hash functions. ESPHome uses the **NNpsk0** pattern to secure its
native API: two anonymous parties perform an ephemeral Diffie-Hellman exchange,
optionally authenticated with a pre-shared key (PSK).

## How ESPHome Noise works

ESPHome devices expose a native API on TCP port **6053**. Before the encrypted
channel is established, the protocol goes through a plaintext handshake phase:

```text
Client                        ESPHome device
  |                                |
  |--- CLIENT_HELLO (indicator=0x01, proto=1) --->|
  |                                |
  |<-- SERVER_HELLO (node name, MAC address) ------|
  |                                |
  |<--- Noise NNpsk0 handshake (PSK required) ---->|
  |                                |
  |<== Encrypted API traffic (ChaCha20-Poly1305) ==>|
```

### Frame structure

**Noise frame** (post-handshake data):

```text
┌────────────┬──────────────────────┬───────────────────────────┬──────────┐
│ Indicator  │  Encrypted size      │  Encrypted payload        │  MAC     │
│  1 byte    │  2 bytes (big-endian)│  variable                 │  16 bytes│
│  0x01      │  NOT encrypted       │  msg type + protobuf data │  AEAD tag│
└────────────┴──────────────────────┴───────────────────────────┴──────────┘
```

**Plaintext frame** (unencrypted alternative, indicator `0x00`):

```text
┌────────────┬──────────────────────┬──────────────────────┬──────────────┐
│ Indicator  │  Payload size VarInt │  Message type VarInt │  Payload     │
│  1 byte    │  1–3 bytes           │  1–2 bytes           │  protobuf    │
└────────────┴──────────────────────┴──────────────────────┴──────────────┘
```

**SERVER_HELLO** (transmitted in plaintext during the handshake phase):

```text
┌────────────┬──────────────────────┬──────────────┬──────────────────────┐
│ Indicator  │  Frame size          │  Protocol    │  Node name\0 MAC\0   │
│  0x01      │  2 bytes (big-endian)│  version (1) │  null-terminated     │
└────────────┴──────────────────────┴──────────────┴──────────────────────┘
```

### Encryption

| Property | Value |
| -------- | ----- |
| Handshake pattern | Noise NNpsk0 |
| Cipher | ChaCha20-Poly1305 (AEAD) |
| Authentication | 16-byte MAC per frame |
| PSK | Pre-shared key configured in ESPHome YAML |
| Port | TCP 6053 (default) |

## Attack surface

- **Pre-authentication device enumeration**: the SERVER_HELLO frame discloses
  the device node name and MAC address in plaintext before PSK verification.
  Any host that can reach port 6053 can fingerprint the device without
  credentials.
- **Missing PSK**: ESPHome allows the native API to operate without a PSK. In
  that configuration the Noise handshake succeeds for any client and no
  authentication is enforced.
- **Plaintext fallback**: if a device is configured for the unencrypted protocol
  variant (indicator `0x00`), all API traffic including authentication data is
  sent in cleartext.
- **Network exposure**: devices accessible from untrusted network segments
  expose their management API to any peer, regardless of PSK configuration.

## Plugins

| Plugin | Command | Summary |
| ------ | ------- | ------- |
| [noise handshake](./noise/handshake.md) | `iottack noise handshake` | Probe the ESPHome Noise handshake and detect information disclosure |

## Typical workflow

1. **Identify the device**: confirm the port is open and the SERVER_HELLO
   reveals the node name and MAC address.

   ```sh
   iottack noise handshake --host 192.168.1.50
   ```

2. **Check for plaintext fallback**: the plugin automatically tests whether
   the device accepts an unencrypted CLIENT_HELLO.

3. **Correlate the MAC address**: match the disclosed MAC against your network
   inventory to confirm device identity.
