//! Test helper binary spawned by `tests/stdio_transport.rs` to exercise the
//! MCP server's real stdio transport. It builds a tiny counter UI, wires an
//! `on_command` that echoes the action, and runs the blocking `run_stdio`
//! loop. The integration test drives it over OS pipes.

use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_mcp::{McpCommand, McpCommandResult, McpServer};

#[derive(Debug, Clone)]
enum Msg {
    Add,
}

fn build_ui() -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Counter");
        c.button("+1")
            .on_click(Msg::Add)
            .ai_action("increment")
            .ai_param("by", "1")
            .ai_description("Increment the counter");
    })
}

fn main() {
    let server = McpServer::new("mcp-e2e", "0.1.0", build_ui, |cmd: McpCommand| {
        if cmd.action == "increment" {
            McpCommandResult::ok(format!("increment {:?}", cmd.params.get("by")))
        } else {
            McpCommandResult::err(format!("unknown action: {}", cmd.action))
        }
    });
    let _ = server.run_stdio();
}
