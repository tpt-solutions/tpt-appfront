//! Clipboard access, surfaced to the hosted page as IPC actions.
//!
//! On Windows this uses the Win32 clipboard directly (`windows-sys`), reading and
//! writing `CF_UNICODETEXT`. Other platforms return an "unavailable" error. The
//! previous implementation used `arboard`, which pulled in `x11rb` and the banned
//! `gethostname`, so macOS clipboard support was lost with it.
//!
//! The page calls `window.__appfront.post("clipboard.read", {})` or
//! `window.__appfront.post("clipboard.write", { text })`. Capability-gated by
//! the [`crate::Acl`]. A page that only needs the clipboard can also use the
//! standard `navigator.clipboard` API, which WebView2 and WKWebView support.

use serde_json::json;

/// Returned by clipboard calls on platforms without a native clipboard backend yet.
#[cfg(not(windows))]
const UNAVAILABLE: &str =
    "native clipboard is not available on this platform yet; use navigator.clipboard in the page";

/// The process-wide clipboard accessor. Each call opens the OS clipboard itself,
/// so the type holds no state and is trivially `Send + Sync`.
#[derive(Default)]
pub struct Clipboard;

impl Clipboard {
    /// Creates a clipboard accessor. Construction never fails.
    pub fn new() -> Self {
        Clipboard
    }

    /// Writes `text` to the system clipboard.
    pub fn write_text(&self, text: &str) -> Result<(), String> {
        imp::write_text(text)
    }

    /// Reads text from the system clipboard (empty string if none is present).
    pub fn read_text(&self) -> Result<String, String> {
        imp::read_text()
    }
}

#[cfg(windows)]
mod imp {
    use std::ptr;

    use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };

    /// `CF_UNICODETEXT`: NUL-terminated UTF-16.
    const CF_UNICODETEXT: u32 = 13;

    /// Opens the clipboard and closes it again when dropped.
    struct OpenGuard;

    impl OpenGuard {
        fn open() -> Result<Self, String> {
            // SAFETY: a null owner window is allowed by OpenClipboard.
            if unsafe { OpenClipboard(0) } == 0 {
                return Err("could not open the clipboard (another app may hold it)".into());
            }
            Ok(OpenGuard)
        }
    }

    impl Drop for OpenGuard {
        fn drop(&mut self) {
            // SAFETY: this guard opened the clipboard, so it is ours to close.
            unsafe { CloseClipboard() };
        }
    }

    pub(super) fn read_text() -> Result<String, String> {
        // SAFETY: format id and clipboard calls have no pointer preconditions here.
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
            return Ok(String::new());
        }
        let _guard = OpenGuard::open()?;
        // SAFETY: the clipboard is open. The handle is owned by the clipboard and
        // must not be freed here.
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if handle == 0 {
            return Err("clipboard has no text data".into());
        }
        // SAFETY: a CF_UNICODETEXT handle is a global memory block; locking gives a
        // pointer valid until the matching unlock.
        let ptr = unsafe { GlobalLock(handle as _) } as *const u16;
        if ptr.is_null() {
            return Err("could not lock clipboard memory".into());
        }
        let mut units = Vec::new();
        // SAFETY: the clipboard text is NUL-terminated, so this stops at the terminator.
        unsafe {
            let mut p = ptr;
            while *p != 0 {
                units.push(*p);
                p = p.add(1);
            }
            GlobalUnlock(handle as _);
        }
        Ok(String::from_utf16_lossy(&units))
    }

    pub(super) fn write_text(text: &str) -> Result<(), String> {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(0);
        let bytes = units.len() * std::mem::size_of::<u16>();

        let _guard = OpenGuard::open()?;
        // SAFETY: the clipboard is open by this guard.
        if unsafe { EmptyClipboard() } == 0 {
            return Err("could not empty the clipboard".into());
        }
        // SAFETY: allocates a movable global block of `bytes` bytes.
        let mem = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
        if mem.is_null() {
            return Err("could not allocate clipboard memory".into());
        }
        // SAFETY: the block was just allocated with room for `bytes`. The lock and copy
        // stay inside it, and the block is unlocked before ownership passes on.
        unsafe {
            let dst = GlobalLock(mem) as *mut u16;
            if dst.is_null() {
                GlobalFree(mem);
                return Err("could not lock clipboard memory".into());
            }
            ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
            GlobalUnlock(mem);
        }
        // SAFETY: on success the clipboard takes ownership of `mem`. On failure we
        // still own it and must free it.
        let set = unsafe { SetClipboardData(CF_UNICODETEXT, mem as HANDLE) };
        if set == 0 {
            // SAFETY: the clipboard rejected the block, so it is still ours to free.
            unsafe { GlobalFree(mem) };
            return Err("could not set clipboard data".into());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::UNAVAILABLE;

    pub(super) fn read_text() -> Result<String, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(super) fn write_text(_text: &str) -> Result<(), String> {
        Err(UNAVAILABLE.to_string())
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
