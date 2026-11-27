//! The GTK top-level window: title, default size, and the close signal.

use std::cell::Cell;
use std::ffi::{c_int, c_void, CStr};

use anyhow::{bail, Result};

use super::sys::Api;

/// Creates a top-level window. `open_windows` is decremented when GTK destroys the window, which
/// is what happens when the user closes it. The main loop exits once the count reaches zero.
pub(super) fn create(
    api: &Api,
    title: &CStr,
    width: u32,
    height: u32,
    open_windows: &'static Cell<usize>,
) -> Result<*mut c_void> {
    // GTK_WINDOW_TOPLEVEL is 0.
    // SAFETY: GTK is initialised by the caller.
    let window = unsafe { (api.gtk_window_new)(0) };
    if window.is_null() {
        bail!("gtk_window_new returned null");
    }
    // SAFETY: `window` is a live GtkWindow, and `title` is NUL-terminated for the call.
    unsafe {
        (api.gtk_window_set_title)(window, title.as_ptr());
        (api.gtk_window_set_default_size)(window, width as c_int, height as c_int);
        super::connect(
            api,
            window,
            c"destroy",
            on_destroy as unsafe extern "C" fn(*mut c_void, *mut c_void) as *const c_void,
            open_windows as *const Cell<usize> as *mut c_void,
        );
    }
    Ok(window)
}

/// GTK's `destroy` signal handler. `data` is the `&'static Cell<usize>` passed to `create`.
unsafe extern "C" fn on_destroy(_window: *mut c_void, data: *mut c_void) {
    // SAFETY: `data` was set in `create` to a leaked `Cell<usize>` that lives for the process.
    let open = unsafe { &*(data as *const Cell<usize>) };
    open.set(open.get().saturating_sub(1));
}
