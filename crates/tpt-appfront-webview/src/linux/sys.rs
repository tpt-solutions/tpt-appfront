//! The GTK 3, GLib/GObject, GIO, JavaScriptCore and WebKitGTK 4.1 functions the Linux backend
//! uses, resolved by name at startup (`docs/linux-webview-backend.md`).
//!
//! **Signatures are not yet checked against headers.** The types below were written from the
//! GTK 3, GLib and WebKitGTK 4.1 C APIs. The development machine has no Linux headers, so the
//! first real check is the `linux-webview` CI job under `xvfb`. A wrong signature is undefined
//! behaviour at the call, not a load error, so each one must match its C declaration exactly.

use std::ffi::{c_char, c_int, c_ulong, c_void};

use super::loader::{LoadError, SharedLibrary};

// GTK 3 (libgtk-3.so.0)

/// `gboolean gtk_init_check(int *argc, char ***argv)`
pub(crate) type GtkInitCheck = unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) -> c_int;
/// `GtkWidget *gtk_window_new(GtkWindowType type)`; `GTK_WINDOW_TOPLEVEL` is 0.
pub(crate) type GtkWindowNew = unsafe extern "C" fn(c_int) -> *mut c_void;
/// `void gtk_window_set_title(GtkWindow *window, const gchar *title)`
pub(crate) type GtkWindowSetTitle = unsafe extern "C" fn(*mut c_void, *const c_char);
/// `void gtk_window_set_default_size(GtkWindow *window, gint width, gint height)`
pub(crate) type GtkWindowSetDefaultSize = unsafe extern "C" fn(*mut c_void, c_int, c_int);
/// `void gtk_container_add(GtkContainer *container, GtkWidget *widget)`
pub(crate) type GtkContainerAdd = unsafe extern "C" fn(*mut c_void, *mut c_void);
/// `void gtk_widget_show_all(GtkWidget *widget)`
pub(crate) type GtkWidgetShowAll = unsafe extern "C" fn(*mut c_void);

// GLib / GObject (libglib-2.0.so.0, libgobject-2.0.so.0)

/// `gulong g_signal_connect_data(gpointer, const gchar*, GCallback, gpointer, GClosureNotify, GConnectFlags)`
pub(crate) type GSignalConnectData = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_void,
    *mut c_void,
    *const c_void,
    c_int,
) -> c_ulong;
/// `gboolean g_main_context_iteration(GMainContext *context, gboolean may_block)`
pub(crate) type GMainContextIteration = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
/// `void g_object_unref(gpointer object)`
pub(crate) type GObjectUnref = unsafe extern "C" fn(*mut c_void);
/// `void g_free(gpointer mem)`
pub(crate) type GFree = unsafe extern "C" fn(*mut c_void);
/// `GBytes *g_bytes_new(gconstpointer data, gsize size)` (copies `data`)
pub(crate) type GBytesNew = unsafe extern "C" fn(*const c_void, usize) -> *mut c_void;
/// `void g_bytes_unref(GBytes *bytes)`
pub(crate) type GBytesUnref = unsafe extern "C" fn(*mut c_void);

// GIO (libgio-2.0.so.0)

/// `GInputStream *g_memory_input_stream_new_from_bytes(GBytes *bytes)`
pub(crate) type GMemoryInputStreamNewFromBytes = unsafe extern "C" fn(*mut c_void) -> *mut c_void;

// JavaScriptCore (libjavascriptcoregtk-4.1.so.0)

/// `gchar *jsc_value_to_string(JSCValue *value)`; the caller frees the result with `g_free`.
pub(crate) type JscValueToString = unsafe extern "C" fn(*mut c_void) -> *mut c_char;

// WebKitGTK 4.1 (libwebkit2gtk-4.1.so.0)

/// `WebKitUserContentManager *webkit_user_content_manager_new(void)`
pub(crate) type WebKitUserContentManagerNew = unsafe extern "C" fn() -> *mut c_void;
/// `WebKitWebContext *webkit_web_context_get_default(void)`
pub(crate) type WebKitWebContextGetDefault = unsafe extern "C" fn() -> *mut c_void;
/// `void webkit_web_context_register_uri_scheme(WebKitWebContext *context, const gchar *scheme,
/// WebKitURISchemeRequestCallback callback, gpointer user_data, GDestroyNotify destroy)`
pub(crate) type WebKitWebContextRegisterUriScheme = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_void,
    *mut c_void,
    *const c_void,
);
/// `const gchar *webkit_uri_scheme_request_get_path(WebKitURISchemeRequest *request)`
pub(crate) type WebKitUriSchemeRequestGetPath = unsafe extern "C" fn(*mut c_void) -> *const c_char;
/// `void webkit_uri_scheme_request_finish(WebKitURISchemeRequest *request, GInputStream *stream,
/// gint64 stream_length, const gchar *content_type)`
pub(crate) type WebKitUriSchemeRequestFinish =
    unsafe extern "C" fn(*mut c_void, *mut c_void, i64, *const c_char);
/// `JSCValue *webkit_javascript_result_get_js_value(WebKitJavascriptResult *js_result)`
pub(crate) type WebKitJavascriptResultGetJsValue = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
/// `WebKitWebView *webkit_web_view_new_with_user_content_manager(WebKitUserContentManager *manager)`
pub(crate) type WebKitWebViewNewWithUcm = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
/// `void webkit_web_view_load_uri(WebKitWebView *web_view, const gchar *uri)`
pub(crate) type WebKitWebViewLoadUri = unsafe extern "C" fn(*mut c_void, *const c_char);
/// `void webkit_web_view_evaluate_javascript(view, const char *script, gssize length, const char *world_name, const char *source_uri, GCancellable *cancellable, GAsyncReadyCallback callback, gpointer user_data)` (WebKitGTK 2.40+)
pub(crate) type WebKitWebViewEvaluateJavascript = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    isize,
    *const c_char,
    *const c_char,
    *mut c_void,
    *const c_void,
    *mut c_void,
);
/// `void webkit_web_view_run_javascript(view, const gchar *script, GCancellable *cancellable, GAsyncReadyCallback callback, gpointer user_data)` (older 2.x)
pub(crate) type WebKitWebViewRunJavascript =
    unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, *const c_void, *mut c_void);
/// `WebKitUserScript *webkit_user_script_new(const gchar *source, WebKitUserContentInjectedFrames, WebKitUserScriptInjectionTime, const gchar * const *allow_list, const gchar * const *block_list)`
pub(crate) type WebKitUserScriptNew = unsafe extern "C" fn(
    *const c_char,
    c_int,
    c_int,
    *const *const c_char,
    *const *const c_char,
) -> *mut c_void;
/// `void webkit_user_content_manager_add_script(WebKitUserContentManager *manager, WebKitUserScript *script)`
pub(crate) type WebKitUserContentManagerAddScript = unsafe extern "C" fn(*mut c_void, *mut c_void);
/// `gboolean webkit_user_content_manager_register_script_message_handler(WebKitUserContentManager *manager, const gchar *name)`
pub(crate) type WebKitUserContentManagerRegisterHandler =
    unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;

/// The loaded libraries and the function pointers the backend calls.
///
/// The libraries are owned here so the function pointers stay valid for as long as this value
/// lives. The backend leaks one `Api` per process, so the pointers stay valid for the whole run.
pub(crate) struct Api {
    pub(crate) gtk_init_check: GtkInitCheck,
    pub(crate) gtk_window_new: GtkWindowNew,
    pub(crate) gtk_window_set_title: GtkWindowSetTitle,
    pub(crate) gtk_window_set_default_size: GtkWindowSetDefaultSize,
    pub(crate) gtk_container_add: GtkContainerAdd,
    pub(crate) gtk_widget_show_all: GtkWidgetShowAll,
    pub(crate) g_signal_connect_data: GSignalConnectData,
    pub(crate) g_main_context_iteration: GMainContextIteration,
    pub(crate) g_object_unref: GObjectUnref,
    pub(crate) g_free: GFree,
    pub(crate) g_bytes_new: GBytesNew,
    pub(crate) g_bytes_unref: GBytesUnref,
    pub(crate) g_memory_input_stream_new_from_bytes: GMemoryInputStreamNewFromBytes,
    pub(crate) jsc_value_to_string: JscValueToString,
    pub(crate) webkit_user_content_manager_new: WebKitUserContentManagerNew,
    pub(crate) webkit_web_context_get_default: WebKitWebContextGetDefault,
    pub(crate) webkit_web_context_register_uri_scheme: WebKitWebContextRegisterUriScheme,
    pub(crate) webkit_uri_scheme_request_get_path: WebKitUriSchemeRequestGetPath,
    pub(crate) webkit_uri_scheme_request_finish: WebKitUriSchemeRequestFinish,
    pub(crate) webkit_javascript_result_get_js_value: WebKitJavascriptResultGetJsValue,
    pub(crate) webkit_web_view_new_with_ucm: WebKitWebViewNewWithUcm,
    pub(crate) webkit_web_view_load_uri: WebKitWebViewLoadUri,
    /// Preferred on WebKitGTK 2.40+. `None` on older versions, which use `run_javascript`.
    pub(crate) webkit_web_view_evaluate_javascript: Option<WebKitWebViewEvaluateJavascript>,
    /// Fallback for WebKitGTK releases before 2.40.
    pub(crate) webkit_web_view_run_javascript: Option<WebKitWebViewRunJavascript>,
    pub(crate) webkit_user_script_new: WebKitUserScriptNew,
    pub(crate) webkit_ucm_add_script: WebKitUserContentManagerAddScript,
    pub(crate) webkit_ucm_register_handler: WebKitUserContentManagerRegisterHandler,
    _libraries: Libraries,
}

/// Keeps the shared libraries mapped. Never read, only held.
#[allow(dead_code)]
struct Libraries {
    glib: SharedLibrary,
    gobject: SharedLibrary,
    gio: SharedLibrary,
    gtk: SharedLibrary,
    jsc: SharedLibrary,
    webkit: SharedLibrary,
}

impl Api {
    /// Opens GTK 3, WebKitGTK 4.1 and their dependencies, and resolves every symbol the backend
    /// uses.
    ///
    /// Fails with a [`LoadError`] that names the package to install when a library or a required
    /// symbol is missing. `evaluate_javascript` and `run_javascript` are optional: at least one
    /// must resolve, and that check is made here.
    pub(crate) fn load() -> Result<Self, LoadError> {
        let glib = SharedLibrary::open("glib-2.0", &["libglib-2.0.so.0"], "libglib2.0-0")?;
        let gobject = SharedLibrary::open("gobject-2.0", &["libgobject-2.0.so.0"], "libglib2.0-0")?;
        let gio = SharedLibrary::open("gio-2.0", &["libgio-2.0.so.0"], "libglib2.0-0")?;
        let gtk = SharedLibrary::open("gtk-3", &["libgtk-3.so.0"], "libgtk-3-0")?;
        let jsc = SharedLibrary::open(
            "javascriptcoregtk-4.1",
            &["libjavascriptcoregtk-4.1.so.0"],
            "libjavascriptcoregtk-4.1-0",
        )?;
        let webkit = SharedLibrary::open(
            "webkit2gtk-4.1",
            &["libwebkit2gtk-4.1.so.0"],
            "libwebkit2gtk-4.1-0",
        )?;

        // SAFETY: each type matches the C declaration of its symbol as written above. The
        // declarations are checked against headers in CI, not on the development machine.
        let api = unsafe {
            Api {
                gtk_init_check: gtk.symbol("gtk_init_check")?,
                gtk_window_new: gtk.symbol("gtk_window_new")?,
                gtk_window_set_title: gtk.symbol("gtk_window_set_title")?,
                gtk_window_set_default_size: gtk.symbol("gtk_window_set_default_size")?,
                gtk_container_add: gtk.symbol("gtk_container_add")?,
                gtk_widget_show_all: gtk.symbol("gtk_widget_show_all")?,
                g_signal_connect_data: gobject.symbol("g_signal_connect_data")?,
                g_main_context_iteration: glib.symbol("g_main_context_iteration")?,
                g_object_unref: gobject.symbol("g_object_unref")?,
                g_free: glib.symbol("g_free")?,
                g_bytes_new: glib.symbol("g_bytes_new")?,
                g_bytes_unref: glib.symbol("g_bytes_unref")?,
                g_memory_input_stream_new_from_bytes: gio
                    .symbol("g_memory_input_stream_new_from_bytes")?,
                jsc_value_to_string: jsc.symbol("jsc_value_to_string")?,
                webkit_user_content_manager_new: webkit
                    .symbol("webkit_user_content_manager_new")?,
                webkit_web_context_get_default: webkit.symbol("webkit_web_context_get_default")?,
                webkit_web_context_register_uri_scheme: webkit
                    .symbol("webkit_web_context_register_uri_scheme")?,
                webkit_uri_scheme_request_get_path: webkit
                    .symbol("webkit_uri_scheme_request_get_path")?,
                webkit_uri_scheme_request_finish: webkit
                    .symbol("webkit_uri_scheme_request_finish")?,
                webkit_javascript_result_get_js_value: webkit
                    .symbol("webkit_javascript_result_get_js_value")?,
                webkit_web_view_new_with_ucm: webkit
                    .symbol("webkit_web_view_new_with_user_content_manager")?,
                webkit_web_view_load_uri: webkit.symbol("webkit_web_view_load_uri")?,
                webkit_web_view_evaluate_javascript: webkit
                    .symbol("webkit_web_view_evaluate_javascript")
                    .ok(),
                webkit_web_view_run_javascript: webkit
                    .symbol("webkit_web_view_run_javascript")
                    .ok(),
                webkit_user_script_new: webkit.symbol("webkit_user_script_new")?,
                webkit_ucm_add_script: webkit.symbol("webkit_user_content_manager_add_script")?,
                webkit_ucm_register_handler: webkit
                    .symbol("webkit_user_content_manager_register_script_message_handler")?,
                _libraries: Libraries {
                    glib,
                    gobject,
                    gio,
                    gtk,
                    jsc,
                    webkit,
                },
            }
        };

        if api.webkit_web_view_evaluate_javascript.is_none()
            && api.webkit_web_view_run_javascript.is_none()
        {
            return Err(LoadError::SymbolMissing {
                library: "webkit2gtk-4.1",
                symbol: "webkit_web_view_evaluate_javascript",
            });
        }
        Ok(api)
    }
}
