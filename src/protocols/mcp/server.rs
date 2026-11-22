use crate::core::{
    CliArg, Plugin, PluginConfig, PluginFinding, PluginMeta, PluginResult, PluginStatus, Severity,
};
use crate::plugin_meta;
use crate::PluginRegistry;
use anyhow::Result;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub struct ServerPlugin {}

impl ServerPlugin {
    pub fn new() -> Self {
        Self {}
    }

    /// Convert a plugin name (`noise/handshake`) to an MCP tool name (`noise-handshake`).
    fn tool_name(plugin_name: &str) -> String {
        plugin_name.replace('/', "-")
    }

    /// Reverse-lookup the plugin name from an MCP tool name by scanning the registry.
    fn plugin_name(tool_name: &str, registry: &PluginRegistry) -> Option<String> {
        registry
            .list()
            .into_iter()
            .find(|n| Self::tool_name(n) == tool_name)
            .map(str::to_owned)
    }

    /// Build the `tools/list` response body.
    fn tools_list(registry: &PluginRegistry) -> Value {
        let tools: Vec<Value> = registry
            .list()
            .iter()
            .filter(|n| !n.starts_with("mcp/"))
            .filter_map(|name| {
                let mut instance = registry.get(name)?;
                let meta = instance.meta();

                let mut properties = serde_json::Map::new();
                for arg in meta.cli_args {
                    let mut prop = serde_json::Map::new();
                    if arg.is_flag {
                        prop.insert("type".to_string(), json!("boolean"));
                    } else {
                        prop.insert("type".to_string(), json!("string"));
                        if let Some(d) = arg.default {
                            prop.insert("default".to_string(), json!(d));
                        }
                    }
                    prop.insert("description".to_string(), json!(arg.help));
                    properties.insert(arg.name.to_string(), Value::Object(prop));
                }

                let required: Vec<&str> = meta
                    .cli_args
                    .iter()
                    .filter(|a| a.default.is_none() && !a.is_flag)
                    .map(|a| a.name)
                    .collect();

                Some(json!({
                    "name": Self::tool_name(name),
                    "description": format!("{}\n\n{}", meta.summary, meta.description),
                    "inputSchema": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                }))
            })
            .collect();

        json!({ "tools": tools })
    }

    /// Execute a plugin for a `tools/call` request and return the MCP result block.
    fn call_tool(tool_name: &str, arguments: &Value, registry: &PluginRegistry) -> Value {
        let Some(plugin_name) = Self::plugin_name(tool_name, registry) else {
            return json!({
                "content": [{"type": "text", "text": format!("unknown tool: {tool_name}")}],
                "isError": true
            });
        };

        let Some(mut plugin) = registry.get(&plugin_name) else {
            return json!({
                "content": [{"type": "text", "text": "plugin instantiation failed"}],
                "isError": true
            });
        };

        let mut config = PluginConfig::new();
        if let Some(obj) = arguments.as_object() {
            for (k, v) in obj {
                let val = match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(b) => b.to_string(),
                    Value::Number(n) => n.to_string(),
                    _ => v.to_string(),
                };
                config.set(k, val);
            }
        }

        match plugin.run(&config) {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                json!({ "content": [{"type": "text", "text": text}] })
            }
            Err(e) => json!({
                "content": [{"type": "text", "text": format!("plugin error: {e}")}],
                "isError": true
            }),
        }
    }

    /// Dispatch one JSON-RPC request and return the response or `None` for notifications.
    fn handle(request: &Value, registry: &PluginRegistry) -> Option<Value> {
        // Notifications have no "id" field, no response required.
        let id = request.get("id")?;
        let method = request["method"].as_str()?;

        let result = match method {
            "initialize" => json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "iottack",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),

            "tools/list" => Self::tools_list(registry),

            "tools/call" => {
                let params = request.get("params")?;
                let tool_name = params["name"].as_str()?;
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Self::call_tool(tool_name, &args, registry)
            }

            "ping" => json!({}),

            _ => {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("method not found: {method}")
                    }
                }))
            }
        };

        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }
}

impl Default for ServerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for ServerPlugin {
    fn meta(&self) -> PluginMeta {
        plugin_meta!(
            summary: "MCP server that exposes all iottack plugins as AI tools",
            description: "Starts a Model Context Protocol (MCP) server on stdio. \
                          Every registered iottack plugin is advertised as an MCP tool \
                          so that AI models can invoke scans directly. \
                          The server speaks JSON-RPC 2.0 over stdin/stdout and \
                          implements the tools/list and tools/call methods.",
            needs_root: false,
            args: &[],
        )
    }

    fn initialize(&mut self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    fn execute(&mut self) -> Result<PluginResult> {
        let mut result = PluginResult::new(self.meta().name);
        let registry = PluginRegistry::new();

        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let reader = BufReader::new(stdin.lock());
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let request: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("parse error: {e}") }
                    });
                    writeln!(writer, "{err}")?;
                    writer.flush()?;
                    continue;
                }
            };

            if let Some(response) = Self::handle(&request, &registry) {
                writeln!(writer, "{response}")?;
                writer.flush()?;
            }
        }

        result.add(PluginFinding::new(
            Severity::Info,
            "MCP server stopped",
            "The stdio transport closed normally.",
        ));
        result.finish(PluginStatus::Pass);
        Ok(result)
    }
}

inventory::submit! {
    crate::PluginFactory(|| Box::new(ServerPlugin::new()) as Box<dyn crate::Plugin>)
}
