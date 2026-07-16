//! End-to-end test of the MCP server's stdio transport: drives `run_stdio`
//! over real OS pipes and asserts the newline-delimited JSON-RPC framing it
//! produces. Complements the unit tests on `handle_request` (which never
//! exercise the actual read-line / write-line transport).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn start_server() -> (std::process::Child, BufReader<std::process::ChildStdout>, std::process::ChildStdin) {
    let exe = env!("CARGO_BIN_EXE_appfront-mcp-e2e-helper");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn mcp e2e helper");
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stdin = child.stdin.take().unwrap();
    (child, stdout, stdin)
}

#[test]
fn stdio_transport_answers_initialize_and_tools_list() {
    let (mut child, mut stdout, mut stdin) = start_server();

    let write_line = |stdin: &mut std::process::ChildStdin, s: &str| {
        stdin.write_all(s.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    };

    write_line(&mut stdin, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains("\"id\":1"));
    assert!(line.contains("\"protocolVersion\":\"2024-11-05\""));
    assert!(line.contains("\"name\":\"mcp-e2e\""));

    write_line(&mut stdin, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains("\"name\":\"query_state\""));
    assert!(line.contains("\"name\":\"navigate\""));
    assert!(line.contains("\"name\":\"increment\""));

    write_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"increment","arguments":{"by":"1"}}}"#,
    );
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains("\"isError\":false"));
    assert!(line.contains("increment"));

    // A notification gets no response line; the next request still answers.
    write_line(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    write_line(&mut stdin, r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_state","arguments":{}}}"#);
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains("\"id\":4"));
    assert!(line.contains("interactive_elements"));

    drop(stdin);
    let _ = child.wait();
}

#[test]
fn stdio_transport_unknown_method_returns_error() {
    let (mut child, mut stdout, mut stdin) = start_server();
    stdin.write_all(r#"{"jsonrpc":"2.0","id":9,"method":"frobnicate"}"#.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains("\"id\":9"));
    assert!(line.contains("\"code\":-32601"));
    drop(stdin);
    let _ = child.wait();
}
