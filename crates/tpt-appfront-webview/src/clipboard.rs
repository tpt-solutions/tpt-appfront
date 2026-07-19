//! Clipboard access, surfaced to the hosted page as IPC actions.
//!
//! Uses [`arboard`] (a clipboard crate with no GTK dependency). The page calls
//! `window.__appfront.post("clipboard.read", {})` or
//! `window.__appfront.post("clipboard.write", { text })`. Capability-gated by
//! the [`crate::Acl`].

use serde_json::json;
use std::sync::Mutex;

/// A process-wide clipboard handle. [`arboard::Clipboard`] is not `Sync`, so we
/// wrap it in a mutex and lazily initialise on first use.
pub struct Clipboard {
    inner: Mutex<Option<arboard::Clipboard>>,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    /// Creates a clipboard accessor (the underlying OS handle is opened lazily
    /// on first read/write so construction never fails).
    pub fn new() -> Self {
        Clipboard {
            inner: Mutex::new(None),
        }
    }

    fn with<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
    {
        let mut slot = self.inner.lock().unwrap();
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        f(slot.as_mut().unwrap()).map_err(|e| e.to_string())
    }

    /// Writes `text` to the system clipboard.
    pub fn write_text(&self, text: &str) -> Result<(), String> {
        self.with(|c| {
            c.set_text(text.to_string())?;
            Ok(())
        })
    }

    /// Reads text from the system clipboard (empty string if none/unavailable).
    pub fn read_text(&self) -> Result<String, String> {
        self.with(|c| c.get_text())
    }
}

/// Handles a clipboard IPC action, returning the reply value if `action` is a
/// clipboard action (and the [`crate::Acl`] permits it).
pub fn handle_clipboard_action(
    clipboard: &Clipboard,
    acl: &crate::Acl,
    action: &str,
    params: &serde_json::Value,
) -> Option<Result<serde_json::Value, String>> {
    acl.capability(action)?;
    match action {
        "clipboard.read" => Some(clipboard.read_text().map(|t| json!({ "text": t }))),
        "clipboard.write" => {
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(clipboard.write_text(&text).map(|()| json!({ "ok": true })))
        }
        _ => None,
    }
}

/// Standard clipboard capabilities for the [`crate::Acl`].
pub fn clipboard_capabilities() -> Vec<crate::Capability> {
    use crate::{Capability, ParamKind, ParamSpec};
    vec![
        Capability {
            action: "clipboard.read".into(),
            params: vec![],
        },
        Capability {
            action: "clipboard.write".into(),
            params: vec![ParamSpec {
                name: "text".into(),
                required: false,
                kind: ParamKind::String,
                default: None,
            }],
        },
    ]
}
