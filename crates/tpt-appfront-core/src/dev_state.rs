//! Dev-only state persistence for hot reload.
//!
//! `tpt-appfront dev` restarts the app on every source change, which resets every
//! signal to its initial value. A signal made with [`crate::Signal::persisted`]
//! keeps its value across those restarts: each write is saved under the signal's
//! name, and the next start reads it back.
//!
//! Persistence is only on when a dev server asks for it:
//!
//! * **Native**: when the `TPT_APPFRONT_DEV_STATE` environment variable names a
//!   file. `tpt-appfront dev` sets it to `target/tpt-appfront-dev-state.json` and
//!   clears the file at the start of each `dev` session.
//! * **wasm32**: in debug builds only, using `sessionStorage`. That survives the
//!   page reload that `trunk serve` triggers, and it is per tab.
//!
//! Release builds and apps run without the dev server never persist, so a
//! persisted signal behaves like a plain signal there.
//!
//! Values are keyed by name. Renaming a signal starts it fresh. A saved value
//! that no longer deserializes to the signal's type falls back to the initial
//! value. Two signals with the same name share one slot.

use serde_json::{Map, Value};

/// Environment variable that names the native dev-state file.
pub const DEV_STATE_ENV: &str = "TPT_APPFRONT_DEV_STATE";

/// Whether [`crate::Signal::persisted`] saves and restores values right now.
pub(crate) fn enabled() -> bool {
    backend::enabled()
}

/// The saved value for `key`, if one exists.
pub(crate) fn load(key: &str) -> Option<Value> {
    backend::read_map().remove(key)
}

/// Saves `value` under `key`, keeping the other saved keys.
pub(crate) fn save(key: &str, value: Value) {
    let mut map = backend::read_map();
    map.insert(key.to_string(), value);
    backend::write_map(&map);
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    use std::fs;
    use std::path::PathBuf;

    use super::{Map, Value, DEV_STATE_ENV};

    fn path() -> Option<PathBuf> {
        std::env::var_os(DEV_STATE_ENV).map(PathBuf::from)
    }

    pub(super) fn enabled() -> bool {
        path().is_some()
    }

    /// Reads the saved map. A missing or unreadable file reads as empty, so a
    /// corrupt file costs the saved values but never stops the app.
    pub(super) fn read_map() -> Map<String, Value> {
        let Some(path) = path() else {
            return Map::new();
        };
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Writes the map through a temporary file and a rename, so a process killed
    /// mid-write (the dev loop kills its child on every change) leaves the
    /// previous complete file behind.
    pub(super) fn write_map(map: &Map<String, Value>) {
        let Some(path) = path() else {
            return;
        };
        let Ok(text) = serde_json::to_string(map) else {
            return;
        };
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    use super::{Map, Value};

    const KEY: &str = "tpt-appfront-dev-state";

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok().flatten()
    }

    pub(super) fn enabled() -> bool {
        cfg!(debug_assertions) && storage().is_some()
    }

    pub(super) fn read_map() -> Map<String, Value> {
        if !enabled() {
            return Map::new();
        }
        storage()
            .and_then(|s| s.get_item(KEY).ok().flatten())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub(super) fn write_map(map: &Map<String, Value>) {
        if !enabled() {
            return;
        }
        if let (Some(s), Ok(text)) = (storage(), serde_json::to_string(map)) {
            let _ = s.set_item(KEY, &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_without_env_var_reads_empty_and_saves_nothing() {
        // The env var is unset in unit tests unless a test sets it, so the
        // backend is off and nothing is read or written.
        std::env::remove_var(DEV_STATE_ENV);
        assert!(!enabled());
        assert!(load("anything").is_none());
        save("anything", Value::from(1));
        assert!(load("anything").is_none());
    }
}
