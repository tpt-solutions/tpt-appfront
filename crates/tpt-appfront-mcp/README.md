# tpt-appfront-mcp

The MCP (Model Context Protocol) server for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Auto-generates one MCP tool per interactive `UITree` node's `AiMeta`
(`action`/`params`/`description`), wired to `tpt_appfront_core::query_state` /
`tpt_appfront_core::navigate_to` plus an app-supplied command handler (mirroring
`tpt-appfront-server`'s `POST /command`). Any AppFront app becomes drivable by
Claude, or any other MCP client, with zero custom integration work.

## Features

- **Auto tool generation** — `McpServer::new(ui, agent_state, on_command)` turns
  each interactive node's `AiMeta` into an MCP tool (`query_state`, `navigate`, plus
  one per node action).
- **JSON-RPC 2.0 over stdio** — newline-delimited, the standard transport for
  local MCP servers (Claude Desktop, editor integrations, `mcp-cli`, …). No async
  runtime.
- **`McpCommand`** — the inbound shape for tool calls that don't match a built-in
  (`action` + `params`), exactly mirroring `tpt_appfront_server::Command`.
- **`run()`** — a blocking stdio loop: one request per line in, one response per
  line out.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-mcp = "0.1"
```

## Example

```rust
use tpt_appfront_mcp::McpServer;

let mut server = McpServer::new(ui, agent_state, |cmd| {
    // `cmd.action` validated against your node AiMeta actions
    Ok(serde_json::json!({ "ok": true }))
});
server.run(); // blocks on stdin/stdout
```

## License

MIT OR Apache-2.0
