//! Desktop webview shell for appfront DOM apps.
//!
//! A thin wrapper around [`wry`] + [`tao`] (the same stack Tauri uses). It hosts
//! the `trunk build` output of an `appfront-dom` app — the OS's own webview
//! (WebView2 / WKWebView / WebKitGTK), with **no bundled Chromium and no npm
//! toolchain** — and bridges DOM events back to native Rust code over a small,
//! allowlisted IPC channel.
//!
//! ## Layout
//!
//! A webview app has two parts:
//!
//! 1. A native *host* binary (this crate) that opens the window and serves a
//!    `dist/` directory produced by `trunk build`.
//! 2. A `appfront-dom` *UI* crate (built with `trunk`) living under `dist/`.
//!
//! ## IPC bridge
//!
//! `appfront-dom` already annotates interactive nodes with
//! `data-ai-action="<name>"` (see `NodeMeta::ai.action`). At load time this
//! crate injects a tiny script that, on any click reaching an element with that
//! attribute, posts `{ "action": name, "params": {...} }` over the webview IPC
//! channel. The host validates `action` against [`WebviewOptions::allowed_actions`]
//! (rejecting anything not on the list — the Electron-style "open bridge"
//! vulnerability) and forwards allowed commands to your `on_command` closure.
//!
//! From custom JS you can also call `window.__appfront.post(action, params)`
//! directly.

use anyhow::Result;
use governor::{Quota, RateLimiter};
use std::borrow::Cow;
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use wry::application::dpi::LogicalSize;
use wry::application::event::{Event, WindowEvent};
use wry::application::event_loop::{ControlFlow, EventLoop};
use wry::application::window::WindowBuilder;
use wry::http::{header::CONTENT_TYPE, Request, Response};
use wry::webview::WebViewBuilder;

/// Options controlling the webview window and the IPC bridge.
pub struct WebviewOptions {
    /// Window title.
    pub title: String,
    /// Window width in logical pixels.
    pub width: u32,
    /// Window height in logical pixels.
    pub height: u32,
    /// Directory containing the `trunk build` output (`index.html` + assets).
    pub dist_dir: PathBuf,
    /// Action names the hosted page is permitted to dispatch back to native.
    /// Anything else is logged and ignored — this is the IPC allowlist that
    /// prevents an open `eval`-style bridge.
    pub allowed_actions: Vec<String>,
    /// Maximum IPC commands accepted per second (also used as the burst
    /// allowance). A misbehaving or compromised hosted page — e.g. a runaway
    /// script or injected content — can otherwise flood `on_command` with no
    /// limit. Unlike `POST /command` on `appfront-server`, this bridge is
    /// local, synchronous, in-process IPC rather than a network route, so
    /// there is no concept of a per-client key to limit by.
    pub max_commands_per_second: u32,
}

/// Script injected before any page script runs. Exposes `window.__appfront`
/// and auto-posts a command when an element carrying `data-ai-action` is
/// clicked, mirroring the AI-schema action model used elsewhere in appfront.
const INIT_SCRIPT: &str = r#"
(function () {
  if (!window.__appfront) {
    window.__appfront = {
      post: function (action, params) {
        params = params || {};
        window.ipc.postMessage(JSON.stringify({ action: action, params: params }));
      },
    };
  }
  document.addEventListener(
    "click",
    function (ev) {
      var el = ev.target;
      while (el && el !== document) {
        if (el.hasAttribute && el.hasAttribute("data-ai-action")) {
          window.__appfront.post(el.getAttribute("data-ai-action"));
          return;
        }
        el = el.parentNode;
      }
    },
    true
  );
})();
"#;

/// Opens the webview window, serves `dist_dir` over the `app://` custom
/// protocol, and runs the native event loop until the window is closed.
///
/// `on_command` is invoked for every IPC message whose `action` is present in
/// `allowed_actions`. Return `Err` from it to log a rejection; the message is
/// otherwise considered handled.
pub fn run<F>(opts: WebviewOptions, on_command: F) -> Result<()>
where
    F: Fn(&str, serde_json::Value) -> std::result::Result<(), String> + 'static,
{
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(&opts.title)
        .with_inner_size(LogicalSize::new(opts.width, opts.height))
        .build(&event_loop)?;

    let dist_dir = opts.dist_dir.clone();
    let allowed: HashSet<String> = opts.allowed_actions.iter().cloned().collect();
    let limiter = new_ipc_rate_limiter(opts.max_commands_per_second);

    let builder = WebViewBuilder::new(window)?
        .with_initialization_script(INIT_SCRIPT)
        .with_custom_protocol("app".to_string(), move |request: &Request<Vec<u8>>| {
            serve(&dist_dir, request)
        })
        .with_ipc_handler(move |_window, message| {
            handle_ipc(&allowed, &limiter, &on_command, &message);
        })
        .with_url("app://localhost/index.html")?;

    let _webview = builder.build()?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Serves a single file from `dist_dir` over the custom protocol.
fn serve(
    dist_dir: &Path,
    request: &Request<Vec<u8>>,
) -> wry::Result<Response<Cow<'static, [u8]>>> {
    let path = request.uri().path();
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    // Confine the resolved path to `dist_dir`. Reject any `..` segment up
    // front, then canonicalize and verify the result is still rooted at
    // `dist_dir` so a crafted URI like `/../../etc/passwd` can't read files
    // outside the app's bundle (path-traversal).
    if rel.split('/').any(|seg| seg == ".." || seg.starts_with("..")) {
        return Ok(not_found());
    }
    let Ok(root) = dist_dir.canonicalize() else {
        return Ok(not_found());
    };
    let Ok(resolved) = dist_dir.join(rel).canonicalize() else {
        return Ok(not_found());
    };
    if !resolved.starts_with(&root) {
        return Ok(not_found());
    }

    match std::fs::read(&resolved) {
        Ok(bytes) => {
            let mime = mime_for(&resolved);
            let resp = Response::builder()
                .status(200)
                .header(CONTENT_TYPE, mime)
                .body(Cow::Owned(bytes))?;
            Ok(resp)
        }
        Err(_) => Ok(not_found()),
    }
}

/// 404 response for the `app://` custom protocol.
fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(404)
        .header(CONTENT_TYPE, "text/plain")
        .body(Cow::Borrowed(b"404 Not Found" as &[u8]))
        .unwrap_or_else(|_| {
            Response::new(Cow::Borrowed(b"404 Not Found" as &[u8]))
        })
}

/// Direct (unkeyed) in-process rate limiter for the IPC bridge — there's no
/// per-client concept here, just one hosted page in one local process.
type IpcRateLimiter = RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::DefaultClock,
>;

/// Builds the IPC rate limiter from [`WebviewOptions::max_commands_per_second`],
/// used as both the steady-state rate and the burst allowance. Falls back to
/// a rate of 1/s if configured as `0`, since a quota of zero is invalid.
fn new_ipc_rate_limiter(max_commands_per_second: u32) -> IpcRateLimiter {
    let n = NonZeroU32::new(max_commands_per_second).unwrap_or(NonZeroU32::new(1).unwrap());
    RateLimiter::direct(Quota::per_second(n).allow_burst(n))
}

/// Mirror of `appfront-server`'s `POST /command` body limit: an IPC message
/// is fully buffered as a `String` before it is JSON-parsed, so an unbounded
/// payload lets a hostile/buggy hosted page allocate arbitrarily much memory in
/// the host process. Reject anything larger up front, before any parse cost.
const MAX_IPC_MESSAGE_BYTES: usize = 16 * 1024;

/// Parses an IPC message, checks the allowlist and rate limit, and dispatches
/// to `on_command`.
fn handle_ipc<F>(allowed: &HashSet<String>, limiter: &IpcRateLimiter, on_command: &F, message: &str)
where
    F: Fn(&str, serde_json::Value) -> std::result::Result<(), String>,
{
    if message.len() > MAX_IPC_MESSAGE_BYTES {
        eprintln!(
            "[appfront-webview] rejecting IPC message: {} bytes exceeds {} byte limit",
            message.len(),
            MAX_IPC_MESSAGE_BYTES
        );
        return;
    }
    let parsed: serde_json::Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[appfront-webview] ignoring malformed IPC message: {e}");
            return;
        }
    };
    let action = match parsed.get("action").and_then(|a| a.as_str()) {
        Some(a) => a.to_string(),
        None => {
            eprintln!("[appfront-webview] ignoring IPC message without `action`");
            return;
        }
    };
    if !allowed.contains(&action) {
        eprintln!(
            "[appfront-webview] rejected IPC action `{action}` (not in allowlist)"
        );
        return;
    }
    if limiter.check().is_err() {
        eprintln!(
            "[appfront-webview] rejected IPC action `{action}` (rate limit exceeded)"
        );
        return;
    }
    let params = parsed.get("params").cloned().unwrap_or(serde_json::Value::Null);
    if let Err(e) = on_command(&action, params) {
        eprintln!("[appfront-webview] command `{action}` failed: {e}");
    }
}

/// Best-effort MIME type from a file extension.
fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html") | Some("htm") => "text/html",
        Some("js") => "text/javascript",
        Some("mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn handle_ipc_rejects_actions_outside_allowlist() {
        let allowed: HashSet<String> = ["increment".to_string()].into_iter().collect();
        let limiter = new_ipc_rate_limiter(10);
        let calls = RefCell::new(Vec::new());
        let on_command = |action: &str, _params: serde_json::Value| -> Result<(), String> {
            calls.borrow_mut().push(action.to_string());
            Ok(())
        };

        handle_ipc(&allowed, &limiter, &on_command, r#"{"action":"reset_everything"}"#);

        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn handle_ipc_rejects_oversized_message_before_parsing() {
        let allowed: HashSet<String> = ["increment".to_string()].into_iter().collect();
        let limiter = new_ipc_rate_limiter(100);
        let calls = RefCell::new(0u32);
        let on_command = |_action: &str, _params: serde_json::Value| -> Result<(), String> {
            *calls.borrow_mut() += 1;
            Ok(())
        };

        // 20 KiB payload (well over the 16 KiB cap) with a valid action inside.
        let big = format!(
            "{{\"action\":\"increment\",\"params\":{{\"x\":\"{}\"}}}}",
            "a".repeat(20 * 1024)
        );
        handle_ipc(&allowed, &limiter, &on_command, &big);

        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn handle_ipc_throttles_once_the_rate_limit_is_exceeded() {
        let allowed: HashSet<String> = ["increment".to_string()].into_iter().collect();
        let limiter = new_ipc_rate_limiter(3);
        let calls = RefCell::new(0u32);
        let on_command = |_action: &str, _params: serde_json::Value| -> Result<(), String> {
            *calls.borrow_mut() += 1;
            Ok(())
        };

        for _ in 0..3 {
            handle_ipc(&allowed, &limiter, &on_command, r#"{"action":"increment"}"#);
        }
        assert_eq!(*calls.borrow(), 3);

        // Burst allowance (3) is now exhausted.
        handle_ipc(&allowed, &limiter, &on_command, r#"{"action":"increment"}"#);
        assert_eq!(*calls.borrow(), 3);
    }
}
