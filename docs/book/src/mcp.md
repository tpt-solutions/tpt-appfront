# MCP server

`tpt-appfront-mcp` exposes a UI to a local agent as MCP tools, over JSON-RPC on stdio.

```rust,ignore
use tpt_appfront_mcp::{McpCommand, McpCommandResult, McpServer};

let server = McpServer::new(
    "shop",
    "0.1.0",
    || cart_view(),                    // builds the UI on demand
    |cmd: McpCommand| run_action(cmd), // runs an action the agent asked for
);
server.run_stdio()?;
```

- One tool is generated for each interactive node that has an AI action, with its parameters.
- `query_state` reads the page as the AI schema describes it.
- `navigate` moves to a route, when the app has a router.
- An action returns an `McpCommandResult`: `ok(message)` or `err(message)`.

The agent sees only what the page exposes. Give each action a description, because `lint_tree` in
`tpt-appfront-test` requires one.
