//! Page-to-native messages. The page calls `window.ipc.postMessage(json)`; the shim below turns
//! that into a WebKit script-message, which lands in `on_message` and then goes through the same
//! `dispatch_ipc` the other platforms use, with the same ACL and rate limit.

use std::cell::Cell;
use std::ffi::{c_void, CStr};
use std::rc::Rc;
use std::sync::Arc;

use crate::app::{ArcLogSink, CommandHandler};
use crate::clipboard::Clipboard;
use crate::ipc_dispatch::dispatch_ipc;
use crate::{Acl, IpcRateLimiter, IpcRejection};

use super::sys::Api;
use super::webview;

/// The name the page posts to, and the handler registered on the manager.
const HANDLER: &CStr = c"appfront";

/// Defines `window.ipc.postMessage` on top of the WebKit message handler. The rest of the init
/// script, shared with the other platforms, expects this function.
pub(super) const SHIM: &str = "window.ipc = { postMessage: function (m) { \
window.webkit.messageHandlers.appfront.postMessage(m); } };\n";

/// Per-app state every window shares.
pub(super) struct IpcEnv {
    pub(super) acl: Acl,
    pub(super) limiter: IpcRateLimiter,
    pub(super) on_command: Rc<CommandHandler>,
    pub(super) app_id: String,
    pub(super) log_sink: ArcLogSink,
    pub(super) clipboard: Arc<Clipboard>,
    pub(super) on_reject: Option<Arc<dyn Fn(IpcRejection) + Send + Sync>>,
}

/// Per-window state for message callbacks. Leaked once per window, like `Api`.
pub(super) struct WindowIpc {
    api: &'static Api,
    env: Rc<IpcEnv>,
    view: Cell<*mut c_void>,
}

impl WindowIpc {
    pub(super) fn new(api: &'static Api, env: Rc<IpcEnv>) -> Self {
        WindowIpc {
            api,
            env,
            view: Cell::new(std::ptr::null_mut()),
        }
    }

    /// Records the view replies are sent to. Set once, after the view exists.
    pub(super) fn set_view(&self, view: *mut c_void) {
        self.view.set(view);
    }
}

/// Registers the message handler on `ucm` and connects it to `ctx`.
pub(super) fn connect(api: &'static Api, ucm: *mut c_void, ctx: &'static WindowIpc) {
    // SAFETY: `ucm` is a live manager. `HANDLER` is NUL-terminated and copied by WebKit.
    unsafe {
        (api.webkit_ucm_register_handler)(ucm, HANDLER.as_ptr());
        super::connect(
            api,
            ucm,
            c"script-message-received::appfront",
            on_message as unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) as *const c_void,
            ctx as *const WindowIpc as *mut c_void,
        );
    }
}

/// The `script-message-received` handler. `result` is a `WebKitJavascriptResult` and `data` is the
/// `WindowIpc` from `connect`.
unsafe extern "C" fn on_message(_ucm: *mut c_void, result: *mut c_void, data: *mut c_void) {
    // SAFETY: `data` is the leaked `WindowIpc` set in `connect`.
    let ctx = unsafe { &*(data as *const WindowIpc) };
    let api = ctx.api;

    // SAFETY: `result` is live for this callback. The value and its string are released below.
    let message = unsafe {
        let value = (api.webkit_javascript_result_get_js_value)(result);
        if value.is_null() {
            return;
        }
        let raw = (api.jsc_value_to_string)(value);
        if raw.is_null() {
            return;
        }
        let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
        (api.g_free)(raw.cast());
        text
    };

    let eval = |script: &str| webview::evaluate(api, ctx.view.get(), script);
    dispatch_ipc(
        &ctx.env.acl,
        &ctx.env.limiter,
        &*ctx.env.on_command,
        &ctx.env.app_id,
        &ctx.env.log_sink,
        &ctx.env.clipboard,
        &eval,
        &ctx.env.on_reject,
        &message,
    );
}
