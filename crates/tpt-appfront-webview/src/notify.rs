//! Native desktop notifications, surfaced to the hosted page as an IPC action.
//!
//! Uses [`notify_rust`] (pure-Rust on Linux/BSD, native frameworks elsewhere).
//! The page calls `window.__appfront.post("notify", { title, body, ... })`;
//! the shell raises a native notification. Capability-gated by the
//! [`crate::Acl`].

use notify_rust::{Notification, Timeout};

/// Raises a desktop notification from the validated IPC params.
///
/// Recognised params: `title` (string), `body` (string), `timeout_ms`
/// (number; omitted or 0 means the OS default), `sound` (bool, macOS only).
pub fn notify(params: &serde_json::Value) -> Result<(), String> {
    let title = params
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("notify requires `title`")?;
    let body = params.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let timeout_ms = params.get("timeout_ms").and_then(|v| v.as_u64());

    let mut n = Notification::new();
    n.summary(title);
    if !body.is_empty() {
        n.body(body);
    }
    if let Some(ms) = timeout_ms {
        if ms > 0 {
            n.timeout(Timeout::Milliseconds(ms as u32));
        }
    }
    #[cfg(target_os = "macos")]
    if params.get("sound").and_then(|v| v.as_bool()).unwrap_or(false) {
        n.sound_name("Ping");
    }
    n.show().map(|_| ()).map_err(|e| e.to_string())
}

/// Returns `true` and performs the notify if `action` is the `notify` action
/// and the [`crate::Acl`] permits it.
pub fn handle_notify_action(
    acl: &crate::Acl,
    action: &str,
    params: &serde_json::Value,
) -> Option<Result<(), String>> {
    if action != "notify" || acl.capability("notify").is_none() {
        return None;
    }
    Some(notify(params))
}

/// Standard `notify` capability for the [`crate::Acl`].
pub fn notify_capability() -> crate::Capability {
    use crate::{Capability, ParamKind, ParamSpec};
    Capability {
        action: "notify".into(),
        params: vec![
            ParamSpec { name: "title".into(), required: true, kind: ParamKind::String, default: None },
            ParamSpec { name: "body".into(), required: false, kind: ParamKind::String, default: None },
            ParamSpec { name: "timeout_ms".into(), required: false, kind: ParamKind::Number, default: None },
            ParamSpec { name: "sound".into(), required: false, kind: ParamKind::Boolean, default: None },
        ],
    }
}
