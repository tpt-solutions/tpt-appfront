//! The GTK main loop. It runs until every window has been destroyed.

use std::cell::Cell;
use std::ptr;

use super::sys::Api;

/// Drives the default GLib main context until `open_windows` reaches zero. Each iteration blocks
/// until there is an event, so the loop uses no CPU while idle. Window destruction and IPC both
/// arrive as events, so the count is checked after each one.
pub(super) fn run(api: &Api, open_windows: &Cell<usize>) {
    while open_windows.get() > 0 {
        // SAFETY: the default context is NULL. `may_block` is TRUE (1).
        unsafe { (api.g_main_context_iteration)(ptr::null_mut(), 1) };
    }
}
