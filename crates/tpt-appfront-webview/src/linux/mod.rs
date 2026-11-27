//! Linux backend: GTK 3 and WebKitGTK 4.1, loaded at runtime with no build-time GTK dependency.
//! The design is in `docs/linux-webview-backend.md`.
//!
//! Limits of this first version: one window per app; no drag and drop, no devtools, no
//! persisted geometry. The first window's `dist_dir` serves `app://`.
//!
//! **Unverified on a real display.** The development machine is Windows, so this code is
//! type-checked for Linux but not run. It is exercised by the `linux-webview` CI job under
//! `xvfb`.

mod ipc;
// Stage 2 loader, exercised by its unit tests and by `Api::load`.
mod loader;
mod main_loop;
mod scheme;
mod sys;
mod webview;
mod window;

use std::cell::Cell;
use std::ffi::{c_void, CStr, CString};
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};

use self::ipc::{IpcEnv, WindowIpc};
use self::sys::Api;
use crate::app::{AppBuilder, CommandHandler};
use crate::clipboard::Clipboard;

/// Connects `callback` to `signal` on `instance`. `callback` must have the C signature the
/// signal expects, and `data` must stay valid for as long as the signal can fire.
///
/// # Safety
/// `instance` must be a live GObject, and both the callback and `data` must match the signal.
pub(crate) unsafe fn connect(
    api: &Api,
    instance: *mut c_void,
    signal: &CStr,
    callback: *const c_void,
    data: *mut c_void,
) {
    // SAFETY: forwarded to the caller's contract.
    unsafe {
        (api.g_signal_connect_data)(instance, signal.as_ptr(), callback, data, ptr::null(), 0);
    }
}

impl AppBuilder {
    /// Runs the Linux event loop until the window closes.
    pub(crate) fn run_native(self, on_command: Rc<CommandHandler>) -> Result<()> {
        let [config] = self.windows.as_slice() else {
            bail!(
                "the Linux webview backend supports exactly one window for now; this app has {}",
                self.windows.len()
            );
        };

        // One `Api` per process. It is leaked so the callbacks can hold `&'static` references.
        let api: &'static Api = Box::leak(Box::new(Api::load().map_err(|e| anyhow!("{e}"))?));

        // SAFETY: called once, before any widget is created.
        if unsafe { (api.gtk_init_check)(ptr::null_mut(), ptr::null_mut()) } == 0 {
            bail!("GTK could not open a display. Set DISPLAY, or run the app under xvfb-run");
        }

        // The scheme must be registered before the first request for `app://`.
        scheme::register(api, config.dist_dir.clone());

        let env = Rc::new(IpcEnv {
            acl: self.acl.clone(),
            limiter: crate::new_ipc_rate_limiter(self.max_commands_per_second),
            on_command,
            app_id: self.app_id.clone(),
            log_sink: self.log_sink.clone(),
            clipboard: Arc::new(Clipboard::new()),
            on_reject: self.on_ipc_rejection.clone(),
        });

        let title = CString::new(config.title.as_str()).unwrap_or_default();
        let open_windows: &'static Cell<usize> = Box::leak(Box::new(Cell::new(1)));
        let window = window::create(api, &title, config.width, config.height, open_windows)?;

        // SAFETY: GTK is initialised; the user-content manager is a fresh GObject.
        let ucm = unsafe { (api.webkit_user_content_manager_new)() };
        let ipc_ctx: &'static WindowIpc = Box::leak(Box::new(WindowIpc::new(api, env)));
        ipc::connect(api, ucm, ipc_ctx);

        let init_script = format!("{}{}", ipc::SHIM, crate::INIT_SCRIPT);
        let view = webview::create(api, ucm, "app://localhost/index.html", &init_script)?;
        ipc_ctx.set_view(view);

        // SAFETY: `window` and `view` are live widgets, and `view` is added once.
        unsafe {
            (api.gtk_container_add)(window, view);
            (api.gtk_widget_show_all)(window);
        }

        main_loop::run(api, open_windows);
        Ok(())
    }
}
