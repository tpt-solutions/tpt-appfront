# tpt-appfront-mcp

The MCP (Model Context Protocol) server for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

It turns an AppFront app's agent API into MCP tools. It builds one tool per
interactive node that has an AI action, plus two built-in tools, and sends each call
to your app. Any MCP client, such as Claude Desktop or an editor integration, can then
drive the app without custom integration code.

## Public API

Re-exported from `src/lib.rs`:

- `McpServer<Msg>`
  - `McpServer::new(name, version, build_ui, on_command)`, where `build_ui` is
    `impl Fn() -> UITree<Msg> + 'static` and `on_command` is
    `impl Fn(McpCommand) -> McpCommandResult + 'static`.
  - `run_stdio(&self) -> std::io::Result<()>` runs the transport. It blocks until stdin
    closes.
- `McpCommand { action: String, params: HashMap<String, serde_json::Value> }`, the
  input to `on_command`. `action` is the name of the tool that was called.
- `McpCommandResult`, with `McpCommandResult::ok(message)` and
  `McpCommandResult::err(message)`. An error result is returned to the client with
  `isError: true`.

## Tools

- `query_state`: returns a JSON snapshot of the current UI from
  `tpt_appfront_core::query_state`.
- `navigate`: takes a required `route` argument and calls
  `tpt_appfront_core::navigate_to`.
- One tool per interactive node whose AI metadata has an action. The tool name is the
  action, and each parameter from the node's metadata becomes a string argument. Calls
  to these tools go to `on_command`.

`build_ui` runs again for each `tools/list` and `query_state` call, so the tools
always reflect the current state.

## Transport

- JSON-RPC 2.0, one message per line, on stdin and stdout. Write logs to stderr,
  because stdout carries the protocol.
- `initialize` reports protocol version `2024-11-05`.
- Lines longer than 16 KiB, and lines that are not valid JSON, are dropped without a
  response.
- No async runtime is needed.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-mcp = "0.1"
```

This crate has no Cargo feature flags.

## Example

```rust
use tpt_appfront_mcp::{McpCommandResult, McpServer};

fn main() -> std::io::Result<()> {
    let server = McpServer::new(
        "my-app",
        "0.1.0",
        || build_ui(), // your app's UITree<Msg>
        |cmd| {
            // `cmd.action` matches a node's AI action name
            McpCommandResult::ok(format!("ran {}", cmd.action))
        },
    );
    server.run_stdio()
}
```

An MCP client launches this binary and talks to it over stdio.

## License

MIT OR Apache-2.0
