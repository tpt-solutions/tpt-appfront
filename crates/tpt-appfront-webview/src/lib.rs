//! Desktop webview shell for appfront DOM apps.
//!
//! A thin wrapper around [`wry`] + [`tao`] (the same stack Tauri uses). It hosts
//! the `trunk build` output of an `tpt-appfront-dom` app — the OS's own webview
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
//! 2. A `tpt-appfront-dom` *UI* crate (built with `trunk`) living under `dist/`.
//!
//! ## IPC bridge
//!
//! `tpt-appfront-dom` already annotates interactive nodes with
//! `data-ai-action="<name>"` (see `NodeMeta::ai.action`). At load time this
//! crate injects a tiny script that, on any click reaching an element with that
//! attribute, posts `{ "action": name, "params": {...} }` over the webview IPC
//! channel. The host validates `action` and its parameters against
//! [`WebviewOptions::acl`] (rejecting anything not granted — and rejecting
//! out-of-contract arguments — the Electron-style "open bridge"
//! vulnerability) and forwards allowed commands to your `on_command` closure.
//!
//! From custom JS you can also call `window.__appfront.post(action, params)`
//! directly.

use anyhow::Result;
use governor::{Quota, RateLimiter};
use std::borrow::Cow;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use wry::http::{header::CONTENT_TYPE, Request, Response};

mod sidecar;
pub use sidecar::{LogSink, SidecarConfig, SidecarSupervisor, Stream};

mod logging;

mod manager;
pub use manager::{AppBuilder, WindowConfig};

mod secret;
pub use secret::{delete_secret, get_secret, set_secret, SecretError};

mod dialog;
pub use dialog::{open_dialog, save_dialog};

mod notify;
pub use notify::notify;

mod clipboard;
pub use clipboard::Clipboard;

mod shortcut;
pub use shortcut::{parse_shortcut, ShortcutError, ShortcutRegistry};

mod deeplink;
pub use deeplink::{register_scheme, DeepLinkDispatcher, DeepLinkReceiver, DeepLinkSender};

mod dragdrop;
pub use dragdrop::{DragDropDispatcher, DragDropEvent};

mod webrtc;
pub use webrtc::{media_capability, MediaKind};

mod window_state;
pub use window_state::WindowGeometry;

mod single_instance;
pub use single_instance::ensure_single_instance;

mod crash;
pub use crash::{install_panic_hook, CrashReporter, SidecarCrash};

#[cfg(feature = "tray")]
mod tray;
#[cfg(feature = "tray")]
pub use tray::{TrayController, TrayEvent, TrayMenuItem};

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
    /// Per-capability ACL governing which actions the hosted page may dispatch
    /// back to native, and what arguments each action accepts. Replaces the
    /// old flat `allowed_actions` allowlist: instead of merely name-matching an
    /// action, the supervisor validates the action *and* its parameters against
    /// this grant, rejecting malformed or out-of-contract IPC.
    pub acl: Acl,
    /// Maximum IPC commands accepted per second (also used as the burst
    /// allowance). A misbehaving or compromised hosted page — e.g. a runaway
    /// script or injected content — can otherwise flood `on_command` with no
    /// limit. Unlike `POST /command` on `tpt-appfront-server`, this bridge is
    /// local, synchronous, in-process IPC rather than a network route, so
    /// there is no concept of a per-client key to limit by.
    pub max_commands_per_second: u32,
}

impl WebviewOptions {
    /// Convenience constructor from a flat list of allowed action names,
    /// preserving the pre-ACL behaviour (every action permitted with no
    /// parameter contract). Prefer building [`WebviewOptions::acl`] directly
    /// for argument validation.
    pub fn with_allowed_actions(
        title: String,
        width: u32,
        height: u32,
        dist_dir: PathBuf,
        allowed_actions: Vec<String>,
        max_commands_per_second: u32,
    ) -> Self {
        let capabilities = allowed_actions
            .into_iter()
            .map(|action| Capability {
                action,
                params: Vec::new(),
            })
            .collect();
        Self {
            title,
            width,
            height,
            dist_dir,
            acl: Acl { capabilities },
            max_commands_per_second,
        }
    }
}

/// A per-capability / per-window access-control list for the IPC bridge.
///
/// An [`Acl`] grants a set of [`Capability`]s. An action is only dispatched if
/// a grant exists for it *and* its parameters satisfy that grant's contract —
/// moving the IPC surface from "anything on this name list can do anything" to
/// "exactly these actions, with exactly these arguments."
#[derive(Debug, Clone, Default)]
pub struct Acl {
    /// Capabilities granted to the window.
    pub capabilities: Vec<Capability>,
}

impl Acl {
    /// Looks up the grant for `action`, if any.
    pub fn capability(&self, action: &str) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.action == action)
    }

    /// Validates an incoming IPC message's parameters against the grant for
    /// `action`. Returns the (possibly defaulted) validated params, or an error
    /// describing why the message is rejected.
    pub fn validate(&self, action: &str, params: &serde_json::Value) -> Result<serde_json::Value, AclError> {
        let cap = self
            .capability(action)
            .ok_or_else(|| AclError::ActionNotGranted(action.to_string()))?;

        let provided = params.as_object().cloned().unwrap_or_default();

        // Reject unknown params to avoid smuggling unexpected fields through.
        for key in provided.keys() {
            if !cap.params.iter().any(|p| &p.name == key) {
                return Err(AclError::UnknownParam {
                    action: action.to_string(),
                    param: key.clone(),
                });
            }
        }

        // Check required params are present and of an accepted type.
        let mut validated = serde_json::Map::new();
        for spec in &cap.params {
            match provided.get(&spec.name) {
                Some(v) => {
                    if !spec.kind.accepts(v) {
                        return Err(AclError::ParamType {
                            action: action.to_string(),
                            param: spec.name.clone(),
                            expected: spec.kind.label().to_string(),
                        });
                    }
                    validated.insert(spec.name.clone(), v.clone());
                }
                None => {
                    if let Some(default) = &spec.default {
                        validated.insert(spec.name.clone(), default.clone());
                    } else if spec.required {
                        return Err(AclError::MissingParam {
                            action: action.to_string(),
                            param: spec.name.clone(),
                        });
                    }
                }
            }
        }

        Ok(serde_json::Value::Object(validated))
    }
}

/// A single granted capability: an `action` name plus the parameter contract
/// the hosted page must satisfy to invoke it.
#[derive(Debug, Clone)]
pub struct Capability {
    /// The action name as it appears in `data-ai-action` / `window.__appfront.post`.
    pub action: String,
    /// Parameter contract. Empty means "no params accepted".
    pub params: Vec<ParamSpec>,
}

/// Declarative description of one accepted IPC parameter.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// Parameter key.
    pub name: String,
    /// Whether the parameter must be present when the action is invoked.
    pub required: bool,
    /// Accepted JSON value kind(s).
    pub kind: ParamKind,
    /// Default value used when the parameter is absent and not required.
    pub default: Option<serde_json::Value>,
}

/// Allowed JSON value kinds for a [`ParamSpec`]. A union of the common scalar
/// kinds plus object/array, so grants can pin down exactly what a capability
/// accepts.
#[derive(Debug, Clone, Copy)]
pub enum ParamKind {
    /// `string`
    String,
    /// `number` (i64 or f64)
    Number,
    /// `boolean`
    Boolean,
    /// `object`
    Object,
    /// `array`
    Array,
    /// Any value kind.
    Any,
}

impl ParamKind {
    /// Whether `v` is an instance of this kind.
    fn accepts(self, v: &serde_json::Value) -> bool {
        match self {
            ParamKind::String => v.is_string(),
            ParamKind::Number => v.is_number(),
            ParamKind::Boolean => v.is_boolean(),
            ParamKind::Object => v.is_object(),
            ParamKind::Array => v.is_array(),
            ParamKind::Any => true,
        }
    }

    /// Human-readable label for error messages.
    fn label(self) -> &'static str {
        match self {
            ParamKind::String => "string",
            ParamKind::Number => "number",
            ParamKind::Boolean => "boolean",
            ParamKind::Object => "object",
            ParamKind::Array => "array",
            ParamKind::Any => "any",
        }
    }
}

/// Why an IPC message was rejected by the [`Acl`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AclError {
    /// No capability is granted for this action.
    ActionNotGranted(String),
    /// A param was supplied that the capability does not declare.
    UnknownParam { action: String, param: String },
    /// A required param was missing and had no default.
    MissingParam { action: String, param: String },
    /// A param was present but of the wrong JSON kind.
    ParamType {
        action: String,
        param: String,
        expected: String,
    },
}

/// Script injected before any page script runs. Exposes `window.__appfront`
/// and auto-posts a command when an element carrying `data-ai-action` is
/// clicked, mirroring the AI-schema action model used elsewhere in appfront.
///
/// The injected `post` accepts an optional `requestId`; replies are delivered
/// to `window.__appfrontResolve(requestId, result)` (installed here) so the page
/// can `await` results from built-in native actions (secret/dialog/clipboard/
/// notify/media).
const INIT_SCRIPT: &str = r#"
(function () {
  if (!window.__appfrontResolvers) { window.__appfrontResolvers = {}; }
  window.__appfrontResolve = function (requestId, result) {
    var r = window.__appfrontResolvers[requestId];
    if (r) { delete window.__appfrontResolvers[requestId]; r(result); }
  };
  if (!window.__appfront) {
    window.__appfront = {
      post: function (action, params, requestId) {
        params = params || {};
        window.ipc.postMessage(JSON.stringify({ action: action, params: params, requestId: requestId }));
        if (requestId) {
          return new Promise(function (resolve) {
            window.__appfrontResolvers[requestId] = resolve;
          });
        }
        return undefined;
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
/// This is a thin convenience wrapper around [`AppBuilder`] for the common
/// single-window case. `on_command` is invoked for every IPC message whose
/// `action` passes the [`WebviewOptions::acl`]. New code should prefer
/// [`AppBuilder`] for multi-window, sidecar, shortcuts, deep-links, etc.
pub fn run<F>(opts: WebviewOptions, on_command: F) -> Result<()>
where
    F: Fn(&str, serde_json::Value) -> std::result::Result<(), String> + 'static,
{
    let app_id = opts
        .title
        .replace([' ', '/', '\\', ':'], "_")
        .to_lowercase();
    AppBuilder::new(&app_id)
        .with_window(WindowConfig::from_options("main", &opts))
        .with_acl(opts.acl)
        .with_max_commands_per_second(opts.max_commands_per_second)
        .run(on_command)
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

/// Maximum IPC message size accepted before parsing. Mirrors the 16 KiB body
/// limit on `appfront-server`'s `POST /command` (Phase 10): an unbounded IPC
/// string would let a hosted page allocate arbitrarily large buffers in the
/// native process purely by posting oversized messages, so we reject anything
/// above this ceiling before `serde_json::from_str` runs. Messages are plain
/// `{action, params, requestId}` JSON, so 16 KiB is ample for any realistic
/// command while keeping the worst-case parse cost bounded.
#[allow(dead_code)]
const MAX_IPC_MESSAGE_BYTES: usize = 16 * 1024;

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

/// Parses an IPC message, checks the ACL and rate limit, and dispatches
/// to `on_command`.
#[cfg_attr(not(test), allow(dead_code))]
fn handle_ipc<F>(acl: &Acl, limiter: &IpcRateLimiter, on_command: &F, message: &str)
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
    let raw_params = parsed.get("params").cloned().unwrap_or(serde_json::Value::Null);
    let validated = match acl.validate(&action, &raw_params) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[appfront-webview] rejected IPC action `{action}`: {e:?}");
            return;
        }
    };
    if limiter.check().is_err() {
        eprintln!(
            "[appfront-webview] rejected IPC action `{action}` (rate limit exceeded)"
        );
        return;
    }
    if let Err(e) = on_command(&action, validated) {
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

    fn cap(action: &str, params: Vec<ParamSpec>) -> Capability {
        Capability {
            action: action.to_string(),
            params,
        }
    }

    #[test]
    fn handle_ipc_rejects_actions_outside_acl() {
        let acl = Acl {
            capabilities: vec![cap("increment", vec![])],
        };
        let limiter = new_ipc_rate_limiter(10);
        let calls = RefCell::new(Vec::new());
        let on_command = |action: &str, _params: serde_json::Value| -> Result<(), String> {
            calls.borrow_mut().push(action.to_string());
            Ok(())
        };

        handle_ipc(&acl, &limiter, &on_command, r#"{"action":"reset_everything"}"#);

        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn handle_ipc_rejects_oversized_messages_before_parsing() {
        let acl = Acl {
            capabilities: vec![cap("increment", vec![])],
        };
        let limiter = new_ipc_rate_limiter(10);
        let calls = RefCell::new(0u32);
        let on_command = |_action: &str, _params: serde_json::Value| -> Result<(), String> {
            *calls.borrow_mut() += 1;
            Ok(())
        };

        // Build a message of exactly `MAX_IPC_MESSAGE_BYTES` (accepted) and one
        // a single byte longer (rejected), by padding the `p` field to the
        // precise length rather than guessing the JSON overhead.
        let suffix = "\"}";
        let prefix = "{\"action\":\"increment\",\"p\":\"";
        let pad_for = |target: usize| "a".repeat(target.saturating_sub(prefix.len() + suffix.len()));

        let within = format!("{prefix}{}{suffix}", pad_for(MAX_IPC_MESSAGE_BYTES));
        assert_eq!(within.len(), MAX_IPC_MESSAGE_BYTES, "within.len()");
        handle_ipc(&acl, &limiter, &on_command, &within);
        assert_eq!(*calls.borrow(), 1);

        let over = format!("{prefix}{}{suffix}", pad_for(MAX_IPC_MESSAGE_BYTES + 1));
        assert_eq!(over.len(), MAX_IPC_MESSAGE_BYTES + 1, "over.len()");
        handle_ipc(&acl, &limiter, &on_command, &over);
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn handle_ipc_throttles_once_the_rate_limit_is_exceeded() {
        let acl = Acl {
            capabilities: vec![cap("increment", vec![])],
        };
        let limiter = new_ipc_rate_limiter(3);
        let calls = RefCell::new(0u32);
        let on_command = |_action: &str, _params: serde_json::Value| -> Result<(), String> {
            *calls.borrow_mut() += 1;
            Ok(())
        };

        for _ in 0..3 {
            handle_ipc(&acl, &limiter, &on_command, r#"{"action":"increment"}"#);
        }
        assert_eq!(*calls.borrow(), 3);

        // Burst allowance (3) is now exhausted.
        handle_ipc(&acl, &limiter, &on_command, r#"{"action":"increment"}"#);
        assert_eq!(*calls.borrow(), 3);
    }

    #[test]
    fn acl_rejects_unknown_params() {
        let acl = Acl {
            capabilities: vec![cap("increment", vec![])],
        };
        let limiter = new_ipc_rate_limiter(10);
        let calls = RefCell::new(0u32);
        let on_command = |_action: &str, _params: serde_json::Value| -> Result<(), String> {
            *calls.borrow_mut() += 1;
            Ok(())
        };

        handle_ipc(
            &acl,
            &limiter,
            &on_command,
            r#"{"action":"increment","params":{"evil":1}}"#,
        );
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn acl_enforces_param_type_and_default() {
        let acl = Acl {
            capabilities: vec![cap(
                "open",
                vec![
                    ParamSpec {
                        name: "id".to_string(),
                        required: true,
                        kind: ParamKind::String,
                        default: None,
                    },
                    ParamSpec {
                        name: "max".to_string(),
                        required: false,
                        kind: ParamKind::Number,
                        default: Some(serde_json::json!(10)),
                    },
                ],
            )],
        };
        let limiter = new_ipc_rate_limiter(10);
        let calls = RefCell::new(Vec::new());

        handle_ipc(
            &acl,
            &limiter,
            &|action: &str, params: serde_json::Value| -> Result<(), String> {
                calls.borrow_mut().push((action.to_string(), params));
                Ok(())
            },
            r#"{"action":"open"}"#,
        );
        assert!(calls.borrow().is_empty());

        // Wrong type for `id` -> rejected.
        handle_ipc(
            &acl,
            &limiter,
            &|action: &str, params: serde_json::Value| -> Result<(), String> {
                calls.borrow_mut().push((action.to_string(), params));
                Ok(())
            },
            r#"{"action":"open","params":{"id":5}}"#,
        );
        assert!(calls.borrow().is_empty());

        // Valid: id provided, max defaults to 10.
        handle_ipc(
            &acl,
            &limiter,
            &|action: &str, params: serde_json::Value| -> Result<(), String> {
                calls.borrow_mut().push((action.to_string(), params));
                Ok(())
            },
            r#"{"action":"open","params":{"id":"abc"}}"#,
        );
        let captured = calls.borrow();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "open");
        assert_eq!(captured[0].1["id"], "abc");
        assert_eq!(captured[0].1["max"], 10);
    }
}
