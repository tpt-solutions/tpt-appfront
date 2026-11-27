//! The `app://` URI scheme. WebKit asks for a path; the same `serve` function the other platforms
//! use answers it from `dist_dir`, so path-traversal checks live in one place.

use std::borrow::Cow;
use std::ffi::{c_void, CStr, CString};
use std::path::PathBuf;
use std::ptr;

use http::header::CONTENT_TYPE;
use http::{Request, Response};

use super::sys::Api;

/// What the callback needs. Leaked once per process, like `Api`.
struct SchemeCtx {
    api: &'static Api,
    dist_dir: PathBuf,
}

/// Registers `app://` on the default web context, serving files from `dist_dir`.
pub(super) fn register(api: &'static Api, dist_dir: PathBuf) {
    let ctx: &'static SchemeCtx = Box::leak(Box::new(SchemeCtx { api, dist_dir }));
    let scheme = c"app";
    // SAFETY: the default context exists once GTK is initialised. `ctx` lives for the process,
    // and the destroy notify is NULL because it must not be freed.
    unsafe {
        let context = (api.webkit_web_context_get_default)();
        (api.webkit_web_context_register_uri_scheme)(
            context,
            scheme.as_ptr(),
            on_request as unsafe extern "C" fn(*mut c_void, *mut c_void) as *const c_void,
            ctx as *const SchemeCtx as *mut c_void,
            ptr::null(),
        );
    }
}

/// WebKit's URI-scheme callback. `request` is a `WebKitURISchemeRequest`, and `data` is the
/// `SchemeCtx` from `register`.
unsafe extern "C" fn on_request(request: *mut c_void, data: *mut c_void) {
    // SAFETY: `data` is the leaked `SchemeCtx` set in `register`.
    let ctx = unsafe { &*(data as *const SchemeCtx) };
    let api = ctx.api;

    // SAFETY: `request` is live for the duration of this callback.
    let path_ptr = unsafe { (api.webkit_uri_scheme_request_get_path)(request) };
    let path = if path_ptr.is_null() {
        "/".to_string()
    } else {
        // SAFETY: WebKit returns a NUL-terminated string that lives until the request finishes.
        unsafe { CStr::from_ptr(path_ptr) }
            .to_string_lossy()
            .into_owned()
    };

    let (body, mime): (Vec<u8>, String) = match Request::builder()
        .uri(format!("app://localhost{path}"))
        .body(Vec::new())
    {
        Ok(req) => match crate::serve(&ctx.dist_dir, &req) {
            Ok(resp) => (body_bytes(&resp), mime_of(&resp)),
            Err(_) => (b"500 Internal Server Error".to_vec(), "text/plain".into()),
        },
        Err(_) => (b"400 Bad Request".to_vec(), "text/plain".into()),
    };

    finish(api, request, &body, &mime);
}

fn body_bytes(resp: &Response<Cow<'static, [u8]>>) -> Vec<u8> {
    resp.body().to_vec()
}

fn mime_of(resp: &Response<Cow<'static, [u8]>>) -> String {
    resp.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string()
}

/// Hands `body` to WebKit as the response for `request`. GBytes copies the data, so the stream
/// owns its own memory and `body` can be dropped on return.
fn finish(api: &Api, request: *mut c_void, body: &[u8], mime: &str) {
    let mime = CString::new(mime).unwrap_or_else(|_| c"application/octet-stream".to_owned());
    // SAFETY: `body` is valid for `body.len()` bytes for the duration of the call, and GBytes
    // copies it. The stream takes its own reference to the bytes, so we drop ours right away.
    unsafe {
        let bytes = (api.g_bytes_new)(body.as_ptr().cast(), body.len());
        let stream = (api.g_memory_input_stream_new_from_bytes)(bytes);
        (api.g_bytes_unref)(bytes);
        (api.webkit_uri_scheme_request_finish)(request, stream, body.len() as i64, mime.as_ptr());
        (api.g_object_unref)(stream);
    }
}
