# tpt-appfront-mcp

MCP server exposing a [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) app's AI-agent API as JSON-RPC tools over stdio.

`McpServer<Msg>` auto-generates one MCP tool per interactive `UITree` node's AI metadata, plus built-in `query_state`/`navigate` tools, wired to `tpt-appfront-core`'s `query_state`/`navigate_to` and an app-supplied command handler. Speaks JSON-RPC 2.0, newline-delimited, over stdio.

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-mcp = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and the [`ai-agent-demo` example](https://github.com/tpt-solutions/tpt-appfront/tree/main/examples/ai-agent-demo).

## License

Apache-2.0
