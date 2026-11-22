# MCP

The [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) is an open
standard that lets AI models discover and invoke external tools over a
JSON-RPC 2.0 channel. iottack implements an MCP **server** so that AI assistants
such as Claude can run any registered iottack plugin directly. No shell
scripting or manual argument passing required.

## How the MCP server works

The server runs on **stdio** (stdin/stdout), which is the standard transport for
local MCP integrations. The AI client launches `iottack mcp server` as a
subprocess and exchanges newline-delimited JSON messages:

```text
AI client                       iottack mcp server
    |                                  |
    |-- initialize ------------------>|
    |<- {protocolVersion, serverInfo} -|
    |                                  |
    |-- tools/list ------------------>|
    |<- [{name, description, schema}] -|
    |                                  |
    |-- tools/call noise-handshake -->|
    |   {host: "192.168.1.50"}         |
    |<- {content: [{type:"text", ...}]}|
    |                                  |
```

Each iottack plugin is advertised as one MCP tool. The tool name is derived from
the plugin name by replacing the `/` separator with `-` (e.g.,
`noise/handshake` -> `noise-handshake`). The tool's input schema is generated
automatically from the plugin's declared `CliArg` list, so required arguments,
optional arguments, defaults and descriptions are always up to date.

### MCP protocol version

iottack targets MCP **2024-11-05**.

### Supported methods

| Method | Description |
| ------ | ----------- |
| `initialize` | Handshake, returns server info and `tools` capability |
| `tools/list` | Returns all registered plugins as MCP tool descriptors |
| `tools/call` | Runs a plugin and returns the JSON-serialised `PluginResult` |
| `ping` | Heartbeat, returns an empty result |

Notifications (messages without an `id` field) are silently ignored per the
JSON-RPC 2.0 specification.

## Setup

### Claude Desktop

Add the following block to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "iottack": {
      "command": "iottack",
      "args": ["mcp", "server"]
    }
  }
}
```

### Claude Code

```json
{
  "mcpServers": {
    "iottack": {
      "command": "iottack",
      "args": ["mcp", "server"],
      "type": "stdio"
    }
  }
}
```

After restarting the client, `iottack` tools appear in the tool list and can be
called by name from any conversation.

## Plugins

| Plugin | Command | Summary |
| ------ | ------- | ------- |
| [mcp server](./mcp/server.md) | `iottack mcp server` | Start an MCP stdio server that exposes all iottack plugins as AI tools |

## Example AI interaction

Once connected, an AI assistant can run a scan with no manual intervention:

```text
User: Scan 192.168.1.50 for ESPHome API exposure.

Claude: [calls noise-handshake with host=192.168.1.50]

Result:
{
  "plugin": "noise/handshake",
  "status": "Pass",
  "findings": [
    {
      "severity": "medium",
      "title": "Pre-authentication information disclosure",
      "description": "The ESPHome SERVER_HELLO frame transmits the device node name
                      and MAC address in plaintext before the Noise NNpsk0 handshake
                      completes.",
      "evidence": "node_name=\"living-room-sensor\"  mac_address=\"AA:BB:CC:DD:EE:FF\""
    }
  ]
}
```
