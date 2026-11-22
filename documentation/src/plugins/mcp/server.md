# mcp server

Starts a Model Context Protocol (MCP) server on stdio. Every registered
`iottack` plugin is advertised as an MCP tool so that AI models such as Claude
can invoke scans directly from a conversation.

## What it does

On startup the server enters a read loop on stdin. For each newline-delimited
JSON-RPC 2.0 request it:

1. **`initialize`**: replies with the `iottack` server info and declares the
   `tools` capability.
2. **`tools/list`**: iterates all plugins in the registry (excluding itself)
   and returns a descriptor for each: tool name, description, and an input
   schema derived from the plugin's `CliArg` declarations.
3. **`tools/call`**: builds a `PluginConfig` from the supplied arguments,
   calls `plugin.run()` and returns the JSON-serialized `PluginResult` as MCP
   tool content.
4. **`ping`**: returns an empty result.

The server exits cleanly when stdin closes.

## Tool naming

Plugin names use a `/` separator (`noise/handshake`, `http/banner`). MCP tool
names replace `/` with `-` (`noise-handshake`, `http-banner`). The reverse
mapping is resolved by scanning the registry, so no manual mapping table is
required.

## Usage

The plugin is not meant to be invoked directly by a human. Configure your MCP
client to launch it as a subprocess:

```sh
iottack mcp server
```

### Options

This plugin accepts no arguments. All configuration is passed per-call in the
`tools/call` arguments object.

| Flag | Short | Default | Description |
| ---- | ----- | ------- | ----------- |
| `--output <FILE>` | `-o` | - | Save the final PluginResult to a file |
| `--format <FORMAT>` | `-f` | `text` | `text` or `json` |
| `--verbose` | `-v` | - | Increase log verbosity |

## Wire format examples

**Initialize:**

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"claude","version":"1.0"}}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"iottack","version":"0.3.0"}}}
```

**List tools (excerpt):**

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

```json
{"jsonrpc":"2.0","id":2,"result":{"tools":[
  {"name":"noise-handshake","description":"ESPHome Noise protocol handshake probe\n\n...","inputSchema":{"type":"object","properties":{"host":{"type":"string","description":"Target host (IP or hostname)"},"port":{"type":"string","default":"6053","description":"Native API port"},"timeout":{"type":"string","default":"5000","description":"Connection timeout in milliseconds"}},"required":["host"]}},
  {"name":"http-banner","description":"...","inputSchema":{...}}
]}}
```

**Call a tool:**

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"noise-handshake","arguments":{"host":"192.168.1.50"}}}
```

```json
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{\n  \"plugin\": \"noise/handshake\",\n  \"status\": \"Pass\",\n  \"findings\": [...]\n}"}]}}
```

## Setup

See the [MCP overview](../mcp.md) for client configuration examples.
