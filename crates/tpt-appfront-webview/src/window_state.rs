//! Persisted window state (position/size) across relaunches.
//!
//! On close the shell writes the window's outer position + inner size to a
//! small JSON file under the app's data dir; on launch it reads it back so the
//! window reopens where the user left it. Stored per-window by an id.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A persisted window geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowGeometry {
    /// Outer position X in physical pixels (may be absent on first run).
    pub x: Option<i32>,
    /// Outer position Y in physical pixels.
    pub y: Option<i32>,
    /// Inner width in logical pixels.
    pub width: u32,
    /// Inner height in logical pixels.
    pub height: u32,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        WindowGeometry {
            x: None,
            y: None,
            width: 800,
            height: 600,
        }
    }
}

fn state_dir(app_id: &str) -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("appfront")
        .join(app_id)
        .join("window_state")
}

fn state_path(app_id: &str, window_id: &str) -> PathBuf {
    state_dir(app_id).join(format!("{window_id}.json"))
}

/// Loads persisted geometry for `window_id`, or [`WindowGeometry::default`] if
/// none exists / can't be read.
pub fn load(app_id: &str, window_id: &str) -> WindowGeometry {
    let path = state_path(app_id, window_id);
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => WindowGeometry::default(),
    }
}

/// Persists `geo` for `window_id`. Errors are non-fatal (best-effort).
pub fn save(app_id: &str, window_id: &str, geo: &WindowGeometry) {
    let path = state_path(app_id, window_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(geo) {
        let _ = std::fs::write(&path, json);
    }
}

/// Removes persisted state for `window_id` (e.g. on reset).
#[allow(dead_code)]
pub fn clear(app_id: &str, window_id: &str) {
    let _ = std::fs::remove_file(state_path(app_id, window_id));
}

/// Helper to derive a `Path` reference for callers needing the raw path.
pub fn _path(app_id: &str, window_id: &str) -> PathBuf {
    state_path(app_id, window_id)
}
