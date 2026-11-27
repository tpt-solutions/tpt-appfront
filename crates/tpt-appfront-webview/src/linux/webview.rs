//! The WebKitGTK view: creation, the injected start-of-document script, and JavaScript evaluation.

use std::ffi::{c_void, CString};
use std::ptr;

use anyhow::{anyhow, bail, Result};

use super::sys::Api;

/// `WEBKIT_USER_CONTENT_INJECT_ALL_FRAMES` (0) and `WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START`
/// (0), from the WebKitGTK 4.1 enums.
const INJECT_ALL_FRAMES: i32 = 0;
const INJECT_AT_DOCUMENT_START: i32 = 0;

/// Creates a view on `ucm`, injects `init_script` at document start, and loads `uri`.
pub(super) fn create(
    api: &Api,
    ucm: *mut c_void,
    uri: &str,
    init_script: &str,
) -> Result<*mut c_void> {
    let source = CString::new(init_script).map_err(|e| anyhow!("init script: {e}"))?;
    // SAFETY: `source` is NUL-terminated and outlives the call. The script object is referenced by
    // the manager once added, so our reference is released after `add_script`.
    unsafe {
        let script = (api.webkit_user_script_new)(
            source.as_ptr(),
            INJECT_ALL_FRAMES,
            INJECT_AT_DOCUMENT_START,
            ptr::null(),
            ptr::null(),
        );
        if script.is_null() {
            bail!("webkit_user_script_new returned null");
        }
        (api.webkit_ucm_add_script)(ucm, script);
        (api.g_object_unref)(script);
    }

    // SAFETY: `ucm` is a live manager, and the view takes its own reference to it.
    let view = unsafe { (api.webkit_web_view_new_with_ucm)(ucm) };
    if view.is_null() {
        bail!("webkit_web_view_new_with_user_content_manager returned null");
    }
    let uri = CString::new(uri).map_err(|e| anyhow!("uri: {e}"))?;
    // SAFETY: `view` is a live WebKitWebView, and `uri` is NUL-terminated.
    unsafe { (api.webkit_web_view_load_uri)(view, uri.as_ptr()) };
    Ok(view)
}

/// Runs `script` in the page. The result is discarded, so this is fire-and-forget.
pub(super) fn evaluate(api: &Api, view: *mut c_void, script: &str) {
    let Ok(script) = CString::new(script) else {
        // A script with an interior NUL cannot be passed to C. Drop it rather than truncate it.
        return;
    };
    // SAFETY: `view` is a live WebKitWebView. The length `-1` tells WebKit the string is
    // NUL-terminated. Callbacks are NULL, so nothing needs to outlive the call.
    unsafe {
        if let Some(eval) = api.webkit_web_view_evaluate_javascript {
            eval(
                view,
                script.as_ptr(),
                -1,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
            );
        } else if let Some(run) = api.webkit_web_view_run_javascript {
            run(
                view,
                script.as_ptr(),
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
            );
        }
    }
}
