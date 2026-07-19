//! MCP (Model Context Protocol) server exposing the Phase 10 agent API —
//! auto-generates one MCP tool per interactive `UITree` node's `AiMeta`
//! (`action`/`params`/`description`), wired to
//! [`tpt_appfront_core::query_state`]/[`tpt_appfront_core::navigate_to`] plus an
//! app-supplied command handler (mirroring `tpt-appfront-server`'s
//! `POST /command`), so any AppFront app is drivable by Claude or any other
//! MCP client with zero custom integration work.
//!
//! Speaks JSON-RPC 2.0 over stdio, newline-delimited — the same transport
//! used by every local MCP server (Claude Desktop, editor integrations,
//! `mcp-cli`, ...). No async runtime: one request per line in, one response
//! per line out.

use tpt_appfront_core::{query_state, UITree};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

/// An inbound instruction translated from an MCP `tools/call` invocation
/// whose `name` didn't match a built-in tool (`query_state`/`navigate`).
/// `action` is the tool name (== the originating node's `AiMeta::action`)
/// and `params` are the call's `arguments`, exactly mirroring
/// `tpt_appfront_server::Command`.
#[derive(Debug, Clone)]
pub struct McpCommand {
    pub action: String,
    pub params: HashMap<String, Value>,
}

/// Result of executing an [`McpCommand`], reported back to the MCP client
/// as tool-call content (`ok: false` is surfaced as `isError: true`).
#[derive(Debug, Clone)]
pub struct McpCommandResult {
    pub ok: bool,
    pub message: String,
}

impl McpCommandResult {
    pub fn ok(message: impl Into<String>) -> Self {
        McpCommandResult { ok: true, message: message.into() }
    }

    pub fn err(message: impl Into<String>) -> Self {
        McpCommandResult { ok: false, message: message.into() }
    }
}

/// An MCP server for one AppFront app. Rebuilds the `UITree` fresh (via
/// `build_ui`) on every `tools/list`/`query_state` call so tool discovery
/// and state snapshots always reflect current app state — the same
/// immediate-mode pattern `tpt-appfront-canvas` uses.
pub struct McpServer<Msg> {
    name: String,
    version: String,
    build_ui: Box<dyn Fn() -> UITree<Msg>>,
    on_command: Box<dyn Fn(McpCommand) -> McpCommandResult>,
}

impl<Msg> McpServer<Msg> {
    /// `on_command` is where the app interprets a tool call's `action`/
    /// `params` against its own state — typically by calling
    /// [`tpt_appfront_core::trigger_event`] or updating a `Signal` directly.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        build_ui: impl Fn() -> UITree<Msg> + 'static,
        on_command: impl Fn(McpCommand) -> McpCommandResult + 'static,
    ) -> Self {
        McpServer {
            name: name.into(),
            version: version.into(),
            build_ui: Box::new(build_ui),
            on_command: Box::new(on_command),
        }
    }

    /// Runs the stdio transport loop: reads one JSON-RPC request per line
    /// from stdin, writes one JSON-RPC response per line to stdout. Blocks
    /// until stdin is closed (the standard way an MCP client shuts down a
    /// stdio-transport server).
    pub fn run_stdio(&self) -> io::Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut out = stdout.lock();

        for line in stdin.lock().lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(request) = serde_json::from_str::<Value>(trimmed) else {
                continue; // malformed frame — drop it rather than crash the server
            };
            if let Some(response) = self.handle_request(&request) {
                writeln!(out, "{}", serde_json::to_string(&response)?)?;
                out.flush()?;
            }
        }
        Ok(())
    }

    /// Handles one parsed JSON-RPC request, returning `None` for
    /// notifications (no `id`, no response expected).
    fn handle_request(&self, request: &Value) -> Option<Value> {
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");

        match method {
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": { "name": self.name, "version": self.version },
                    "capabilities": { "tools": {} },
                }
            })),
            "notifications/initialized" | "notifications/cancelled" => None,
            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": self.tool_defs() }
            })),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments: HashMap<String, Value> = params
                    .get("arguments")
                    .and_then(Value::as_object)
                    .map(|o| o.clone().into_iter().collect())
                    .unwrap_or_default();
                Some(self.call_tool(name, arguments, id))
            }
            _ => id.map(|id| {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("method not found: {method}") }
                })
            }),
        }
    }

    /// One MCP tool per interactive element that carries an `ai_action`
    /// (via [`query_state`]), plus two always-available built-ins:
    /// `query_state` (read-only UI snapshot) and `navigate` (route change).
    fn tool_defs(&self) -> Vec<Value> {
        let ui = (self.build_ui)();
        let state = query_state(&ui);

        let mut tools = vec![
            json!({
                "name": "query_state",
                "description": "Return a structured snapshot of the current UI: interactive elements, data elements, and the current route.",
                "inputSchema": { "type": "object", "properties": {} },
            }),
            json!({
                "name": "navigate",
                "description": "Navigate the app to a different route.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "route": { "type": "string", "description": "Route path, e.g. \"/dashboard\"" }
                    },
                    "required": ["route"],
                },
            }),
        ];

        for el in &state.interactive_elements {
            let Some(action) = &el.action else { continue };
            let mut properties = serde_json::Map::new();
            for (key, example) in &el.params {
                properties.insert(
                    key.clone(),
                    json!({ "type": "string", "description": format!("e.g. {example}") }),
                );
            }
            let description = el.description.clone().unwrap_or_else(|| {
                format!(
                    "Invoke the \"{}\" {}",
                    el.label.clone().unwrap_or_default(),
                    el.kind
                )
            });
            tools.push(json!({
                "name": action,
                "description": description,
                "inputSchema": { "type": "object", "properties": Value::Object(properties) },
            }));
        }

        tools
    }

    fn call_tool(&self, name: &str, arguments: HashMap<String, Value>, id: Option<Value>) -> Value {
        let (is_error, text) = match name {
            "query_state" => {
                let ui = (self.build_ui)();
                let state = query_state(&ui);
                (false, serde_json::to_string_pretty(&state).unwrap_or_default())
            }
            "navigate" => match arguments.get("route").and_then(Value::as_str) {
                Some(route) => {
                    tpt_appfront_core::navigate_to(route);
                    (false, format!("navigated to {route}"))
                }
                None => (true, "missing required `route` argument".to_string()),
            },
            _ => {
                let result = (self.on_command)(McpCommand {
                    action: name.to_string(),
                    params: arguments,
                });
                (!result.ok, result.message)
            }
        };

        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": text }],
                "isError": is_error,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_appfront_core::ContainerBuilder;

    #[derive(Debug, Clone)]
    enum TestMsg {
        Add,
    }

    fn sample_ui() -> UITree<TestMsg> {
        UITree::container(|c: &mut ContainerBuilder<TestMsg>| {
            c.heading(1, "Tasks");
            c.button("Add")
                .on_click(TestMsg::Add)
                .ai_action("add_task")
                .ai_param("title", "Buy milk")
                .ai_description("Add a new task");
        })
    }

    fn test_server() -> McpServer<TestMsg> {
        McpServer::new("test-app", "0.1.0", sample_ui, |cmd| {
            if cmd.action == "add_task" {
                McpCommandResult::ok(format!("added task {:?}", cmd.params.get("title")))
            } else {
                McpCommandResult::err(format!("unknown action: {}", cmd.action))
            }
        })
    }

    #[test]
    fn initialize_returns_server_info() {
        let server = test_server();
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
        let resp = server.handle_request(&req).unwrap();
        assert_eq!(resp["result"]["serverInfo"]["name"], "test-app");
        assert_eq!(resp["result"]["capabilities"]["tools"], json!({}));
    }

    #[test]
    fn notifications_get_no_response() {
        let server = test_server();
        let req = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(server.handle_request(&req).is_none());
    }

    #[test]
    fn tools_list_includes_builtins_and_ai_actions() {
        let server = test_server();
        let req = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        let resp = server.handle_request(&req).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"query_state"));
        assert!(names.contains(&"navigate"));
        assert!(names.contains(&"add_task"));

        let add_task = tools.iter().find(|t| t["name"] == "add_task").unwrap();
        assert_eq!(add_task["description"], "Add a new task");
        assert!(add_task["inputSchema"]["properties"]["title"].is_object());
    }

    #[test]
    fn tools_call_query_state_returns_json_snapshot() {
        let server = test_server();
        let req = json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "query_state", "arguments": {} }
        });
        let resp = server.handle_request(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("interactive_elements"));
    }

    #[test]
    fn tools_call_navigate_requires_route() {
        let server = test_server();
        let req = json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "navigate", "arguments": {} }
        });
        let resp = server.handle_request(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn tools_call_forwards_app_action_to_on_command() {
        let server = test_server();
        let req = json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": { "name": "add_task", "arguments": { "title": "Buy milk" } }
        });
        let resp = server.handle_request(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Buy milk"));
    }

    #[test]
    fn unknown_method_returns_json_rpc_error() {
        let server = test_server();
        let req = json!({"jsonrpc": "2.0", "id": 6, "method": "not/a/real/method"});
        let resp = server.handle_request(&req).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }
}
