//! Native file dialogs, surfaced to the hosted page as IPC actions.
//!
//! Uses [`rfd`] (a thin cross-platform wrapper over the OS picker). The dialog
//! runs synchronously on the IPC thread for simplicity; the page calls
//! `window.__appfront.post("dialog.open", {...})` and receives the chosen path
//! (or `null`) back. Capability-gated by the [`crate::Acl`].

use serde_json::json;

/// Opens a native file picker and returns the selected path(s) as JSON.
///
/// `params` understands: `title` (string), `multiple` (bool), `dir` (bool to
/// pick a directory), `filter_name` (string), `filter_ext` (array of strings).
pub fn open_dialog(params: &serde_json::Value) -> serde_json::Value {
    let title = params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Open");
    let multiple = params.get("multiple").and_then(|v| v.as_bool()).unwrap_or(false);
    let pick_dir = params.get("dir").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut picker = rfd::FileDialog::new().set_title(title);
    if let (Some(name), Some(exts)) = (
        params.get("filter_name").and_then(|v| v.as_str()),
        params.get("filter_ext").and_then(|v| v.as_array()),
    ) {
        let exts: Vec<&str> = exts.iter().filter_map(|e| e.as_str()).collect();
        if !exts.is_empty() {
            picker = picker.add_filter(name, &exts);
        }
    }

    if pick_dir {
        return match picker.pick_folder() {
            Some(p) => json!({ "path": p.to_string_lossy() }),
            None => json!(null),
        };
    }

    if multiple {
        let paths: Vec<String> = picker
            .pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        json!({ "paths": paths })
    } else {
        match picker.pick_file() {
            Some(p) => json!({ "path": p.to_string_lossy() }),
            None => json!(null),
        }
    }
}

/// Opens a native save picker and returns the chosen destination path (or
/// `null` if cancelled).
pub fn save_dialog(params: &serde_json::Value) -> serde_json::Value {
    let title = params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Save");
    let mut picker = rfd::FileDialog::new().set_title(title);
    if let (Some(name), Some(exts)) = (
        params.get("filter_name").and_then(|v| v.as_str()),
        params.get("filter_ext").and_then(|v| v.as_array()),
    ) {
        let exts: Vec<&str> = exts.iter().filter_map(|e| e.as_str()).collect();
        if !exts.is_empty() {
            picker = picker.add_filter(name, &exts);
        }
    }
    if let Some(default) = params.get("file_name").and_then(|v| v.as_str()) {
        picker = picker.set_file_name(default);
    }
    match picker.save_file() {
        Some(p) => json!({ "path": p.to_string_lossy() }),
        None => json!(null),
    }
}

/// Returns `Some(value)` if `action` is a dialog action (and handled),
/// otherwise `None`. `acl` controls which actions are permitted.
pub fn handle_dialog_action(
    acl: &crate::Acl,
    action: &str,
    params: &serde_json::Value,
) -> Option<serde_json::Value> {
    let permitted = acl.capability(action).is_some();
    if !permitted {
        return None;
    }
    match action {
        "dialog.open" => Some(open_dialog(params)),
        "dialog.save" => Some(save_dialog(params)),
        _ => None,
    }
}

/// Standard dialog capabilities for the [`crate::Acl`] (open + save).
pub fn dialog_capabilities() -> Vec<crate::Capability> {
    use crate::{Capability, ParamKind, ParamSpec};
    let params = |_required: bool| {
        vec![
            ParamSpec { name: "title".into(), required: false, kind: ParamKind::String, default: None },
            ParamSpec { name: "multiple".into(), required: false, kind: ParamKind::Boolean, default: None },
            ParamSpec { name: "dir".into(), required: false, kind: ParamKind::Boolean, default: None },
            ParamSpec { name: "filter_name".into(), required: false, kind: ParamKind::String, default: None },
            ParamSpec { name: "filter_ext".into(), required: false, kind: ParamKind::Array, default: None },
            ParamSpec { name: "file_name".into(), required: false, kind: ParamKind::String, default: None },
        ]
    };
    vec![
        Capability { action: "dialog.open".into(), params: params(false) },
        Capability { action: "dialog.save".into(), params: params(false) },
    ]
}
