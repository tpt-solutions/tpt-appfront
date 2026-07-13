# appfront-mcp

MCP server exposing [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) UI state and actions as tools.

`McpServer<Msg>` auto-generates one MCP tool per interactive `UITree` node's `AiMeta` (plus built-in `query_state`/`navigate` tools), wired to `appfront_core::query_state`/`navigate_to` and an app-supplied `on_command` closure. Speaks JSON-RPC 2.0, newline-delimited, over stdio — so any MCP-compatible client (Claude Desktop, an agent SDK, etc.) can drive an AppFront UI as a set of tools.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture.
