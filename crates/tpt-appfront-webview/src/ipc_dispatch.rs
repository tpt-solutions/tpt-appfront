//! IPC dispatch shared by every platform backend: validates a page message against the ACL,
//! rate-limits it, and routes it to a built-in subsystem or the app's `on_command`. Replies go
//! back to the page through the `eval` callback, which each backend supplies (wry on Windows and
//! macOS, WebKitGTK on Linux).

use serde_json::json;

use crate::app::{ArcLogSink, CommandHandler, CommandOutcome};
use crate::clipboard::{self, Clipboard};
use crate::secret::{self, SecretError};
use crate::{dialog, notify, webrtc, Acl};

/// Dispatches a single IPC message: validates against the ACL, rate-limits,
/// then routes to built-in subsystems (secret/dialog/notify/clipboard/media)
/// or the app `on_command`. Replies for built-ins are posted back to the page
/// via `window.__appfrontResolve(requestId, result)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_ipc(
    acl: &Acl,
    limiter: &crate::IpcRateLimiter,
    on_command: &CommandHandler,
    app_id: &str,
    log_sink: &ArcLogSink,
    clipboard: &std::sync::Arc<Clipboard>,
    eval: &dyn Fn(&str),
    on_reject: &Option<std::sync::Arc<dyn Fn(crate::IpcRejection) + Send + Sync>>,
    message: &str,
) {
    // Bound the message before parsing so a hosted page cannot make the native process parse an
    // arbitrarily large payload.
    if message.len() > crate::MAX_IPC_MESSAGE_BYTES {
        log_sink.emit(
            crate::sidecar::Stream::Stderr,
            &format!(
                "[appfront-webview] rejecting IPC message: {} bytes exceeds {} byte limit",
                message.len(),
                crate::MAX_IPC_MESSAGE_BYTES
            ),
        );
        if let Some(h) = on_reject {
            h(crate::IpcRejection::TooLarge {
                bytes: message.len(),
            });
        }
        return;
    }
    let parsed: serde_json::Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] malformed IPC: {e}"),
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::Malformed {
                    error: e.to_string(),
                });
            }
            return;
        }
    };
    let action = match parsed.get("action").and_then(|a| a.as_str()) {
        Some(a) => a.to_string(),
        None => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                "[appfront-webview] IPC without `action`",
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::MissingAction);
            }
            return;
        }
    };
    let raw_params = parsed
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let request_id = parsed.get("requestId").cloned();

    let validated = match acl.validate(&action, &raw_params) {
        Ok(v) => v,
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] rejected `{action}`: {e:?}"),
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::ActionDenied {
                    action,
                    reason: format!("{e:?}"),
                });
            }
            reply(eval, request_id, json!({ "error": format!("{e:?}") }));
            return;
        }
    };

    if limiter.check().is_err() {
        log_sink.emit(
            crate::sidecar::Stream::Stderr,
            &format!("[appfront-webview] rate limit exceeded for `{action}`"),
        );
        if let Some(h) = on_reject {
            h(crate::IpcRejection::RateLimited { action });
        }
        reply(eval, request_id, json!({ "error": "rate limited" }));
        return;
    }

    // Built-in subsystems.
    if let Some(r) = secret::handle_secret_action(app_id, acl, &action, &validated) {
        match r {
            Ok(v) => reply(eval, request_id, v),
            Err(SecretError::NotPermitted) => {
                reply(eval, request_id, json!({ "error": "not permitted" }))
            }
            Err(e) => reply(eval, request_id, json!({ "error": e.to_string() })),
        }
        return;
    }
    if let Some(v) = dialog::handle_dialog_action(acl, &action, &validated) {
        reply(eval, request_id, v);
        return;
    }
    if let Some(r) = notify::handle_notify_action(acl, &action, &validated) {
        match r {
            Ok(()) => reply(eval, request_id, json!({ "ok": true })),
            Err(e) => reply(eval, request_id, json!({ "error": e })),
        }
        return;
    }
    if let Some(r) = clipboard::handle_clipboard_action(clipboard, acl, &action, &validated) {
        match r {
            Ok(v) => reply(eval, request_id, v),
            Err(e) => reply(eval, request_id, json!({ "error": e })),
        }
        return;
    }
    if let Some(v) = webrtc::handle_media_action(acl, &action, &validated) {
        reply(eval, request_id, v);
        return;
    }

    // App-defined action (plus synthetic shortcut/deeplink/filedrop which are
    // produced by the event loop, not IPC, and handled there).
    match on_command(&action, validated.clone()) {
        Ok(CommandOutcome::Reply(value)) => {
            reply(eval, request_id, value);
        }
        Ok(CommandOutcome::NoReply) => {}
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] command `{action}` failed: {e}"),
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::DispatchError { action, error: e });
            }
        }
    }
}

/// Delivers an IPC reply to the page through `eval` (which runs `evaluate_script` on the page),
/// if the original message carried a `requestId`.
fn reply(eval: &dyn Fn(&str), request_id: Option<serde_json::Value>, value: serde_json::Value) {
    let Some(req) = request_id else { return };
    let payload = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
    eval(&format!("window.__appfrontResolve({req}, {payload});"));
}
