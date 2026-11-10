//! End-to-end integration tests that start a real HTTP server and verify
//! the response differs by User-Agent.

use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_server::SmartRouterBuilder;

type Msg = ();

fn test_ui() -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Hello").class("title");
        c.button("Click").ai_action("greet");
    })
}

fn pick_port() -> (u16, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    (port, listener)
}

/// Spins up the server on a background thread; drops it on drop.
struct ServerGuard {
    port: u16,
    _shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ServerGuard {
    fn start(ui: UITree<Msg>) -> Self {
        let router = SmartRouterBuilder::new(ui)
            .title("E2E Test App")
            .description("E2E test for routing")
            .build();
        Self::start_router(router)
    }

    fn start_router(router: tpt_appfront_server::SmartRouter<Msg>) -> Self {
        let (port, listener) = pick_port();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                listener.set_nonblocking(true).expect("set_nonblocking");
                let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
                // `serve_listener` runs until it fails, so stop it through the atomic flag.
                tokio::select! {
                    _ = tpt_appfront_server::serve_listener(router, listener) => {}
                    _ = async move {
                        while !shutdown_clone.load(Ordering::SeqCst) {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    } => {}
                }
            });
        });

        // Wait until the server answers, instead of sleeping for a fixed time. A fixed
        // sleep was flaky under load, because the accept loop starts a moment after bind.
        let url = format!("http://127.0.0.1:{port}/");
        for _ in 0..200 {
            match ureq::get(&url).call() {
                // Any HTTP answer, even an error status, means the server is serving.
                Ok(_) | Err(ureq::Error::Status(..)) => break,
                Err(_) => thread::sleep(Duration::from_millis(25)),
            }
        }

        ServerGuard {
            port,
            _shutdown: shutdown,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self._shutdown.store(true, Ordering::SeqCst);
            // Give it a moment to shut down, then detach.
            let _ = handle.join();
        }
    }
}

#[test]
fn googlebot_gets_semantic_html() {
    let guard = ServerGuard::start(test_ui());
    let url = guard.url("/");

    let resp = ureq::get(&url)
        .set(
            "User-Agent",
            "Googlebot/2.1 (+http://www.google.com/bot.html)",
        )
        .call()
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.into_string().unwrap();
    assert!(body.contains("<!DOCTYPE html>"), "should return full HTML");
    assert!(
        body.contains("<h1 class=\"title\">Hello</h1>"),
        "should contain rendered UI"
    );
}

#[test]
fn normal_browser_gets_wasm_shell() {
    let guard = ServerGuard::start(test_ui());
    let url = guard.url("/");

    let resp = ureq::get(&url)
        .set(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0",
        )
        .call()
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.into_string().unwrap();
    assert!(
        body.contains("import init from '/app.wasm'"),
        "should contain WASM loader"
    );
    assert!(
        !body.contains("data-appfront-id"),
        "bare shell should not have SSR data"
    );
}

#[test]
fn ai_agent_gets_json() {
    let guard = ServerGuard::start(test_ui());
    let url = guard.url("/");

    let resp = ureq::get(&url)
        .set("User-Agent", "GPTBot/1.0")
        .call()
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.into_string().unwrap();
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json.get("jsonld").is_some(),
        "response should have jsonld key"
    );
    assert!(
        json.get("ai_schema").is_some(),
        "response should have ai_schema key"
    );
}

#[test]
fn client_query_param_overrides_ua() {
    let guard = ServerGuard::start(test_ui());
    let url = guard.url("/?client=crawler");

    // Send with a normal browser UA but ?client=crawler override
    let resp = ureq::get(&url)
        .set("User-Agent", "Mozilla/5.0 Chrome/120")
        .call()
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.into_string().unwrap();
    assert!(
        body.contains("<h1"),
        "crawler override should return HTML even with browser UA"
    );
}

/// `POST /command` over a real socket: the JSON body reaches the app handler, and the
/// content-type and body-size rules apply on the wire, not only in the in-process tests.
#[test]
fn command_round_trip_over_real_socket() {
    use tpt_appfront_server::{CommandResponse, SmartRouterBuilder};

    let router = SmartRouterBuilder::new(test_ui())
        .on_command(|cmd| CommandResponse::ok(format!("ran {}", cmd.action)))
        .build();
    let guard = ServerGuard::start_router(router);
    let url = guard.url("/command");

    let ok = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_string(r#"{"action":"greet"}"#)
        .unwrap();
    assert_eq!(ok.status(), 200);
    assert!(ok.into_string().unwrap().contains("ran greet"));

    let wrong_type = ureq::post(&url).send_string(r#"{"action":"greet"}"#);
    match wrong_type {
        Err(ureq::Error::Status(code, _)) => assert_eq!(code, 415),
        other => panic!("expected 415, got {other:?}"),
    }

    let oversized = format!(r#"{{"action":"{}"}}"#, "a".repeat(20 * 1024));
    let too_big = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_string(&oversized);
    match too_big {
        Err(ureq::Error::Status(code, _)) => assert_eq!(code, 413),
        other => panic!("expected 413, got {other:?}"),
    }
}
