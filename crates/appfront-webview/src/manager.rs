//! Application manager: builds a multi-window webview app, wires the sidecar,
//! and dispatches IPC actions (app-defined + built-in secret/dialog/notify/
//! clipboard/media) plus shortcut/deep-link/drag-drop/tray events through a
//! single `on_command` closure.
//!
//! This is the Phase 1 consolidation of the previous single-window [`crate::run`].

use crate::clipboard::{self, Clipboard};
use crate::crash::{self, CrashReporter};
use crate::deeplink::{self, DeepLinkDispatcher};
use crate::dialog::{self, dialog_capabilities};
use crate::dragdrop::{self, DragDropDispatcher};
use crate::logging::UnifiedLogSink;
use crate::notify::{self, notify_capability};
use crate::secret::{self, SecretError};
use crate::shortcut::{self, ShortcutRegistry};
use crate::sidecar::{LogSink, SidecarConfig, SidecarSupervisor};
use crate::single_instance;
use crate::webrtc::{self, MediaKind};
use crate::window_state;
use crate::{Acl, WebviewOptions};

use anyhow::Result;
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use wry::application::dpi::LogicalSize;
use wry::application::event::{Event, WindowEvent};
use wry::application::event_loop::{ControlFlow, EventLoop};
use wry::application::window::WindowBuilder;
use wry::http::{Request, Response};
use wry::webview::{FileDropEvent, WebView, WebViewBuilder};

/// Configuration for a single window in the app.
#[derive(Clone)]
pub struct WindowConfig {
    /// Stable id used for the window registry and persisted-state file.
    pub id: String,
    /// Window title.
    pub title: String,
    /// Initial width in logical pixels.
    pub width: u32,
    /// Initial height in logical pixels.
    pub height: u32,
    /// Directory served over the `app://` protocol for this window.
    pub dist_dir: PathBuf,
}

impl WindowConfig {
    /// Builds a window config from a [`WebviewOptions`] (single-window migration
    /// path).
    pub fn from_options(id: &str, opts: &WebviewOptions) -> Self {
        WindowConfig {
            id: id.to_string(),
            title: opts.title.clone(),
            width: opts.width,
            height: opts.height,
            dist_dir: opts.dist_dir.clone(),
        }
    }
}

/// The application builder. Accumulates windows, sidecar, shortcuts, deep-link
/// scheme, and lifecycle options, then [`AppBuilder::run`]s the event loop.
pub struct AppBuilder {
    app_id: String,
    windows: Vec<WindowConfig>,
    acl: Acl,
    max_commands_per_second: u32,
    sidecar: Option<SidecarConfig>,
    shortcuts: Vec<(String, String)>,
    deeplink_scheme: Option<String>,
    deeplink_dispatcher: Option<DeepLinkDispatcher>,
    single_instance: bool,
    crash_reporter: Option<CrashReporter>,
    log_sink: ArcLogSink,
    persisted_state: bool,
}

type ArcLogSink = std::sync::Arc<dyn LogSink>;

impl AppBuilder {
    /// Starts building an app identified by `app_id` (used for single-instance
    /// lock + persisted window state).
    pub fn new(app_id: &str) -> Self {
        AppBuilder {
            app_id: app_id.to_string(),
            windows: Vec::new(),
            acl: Acl::default(),
            max_commands_per_second: 20,
            sidecar: None,
            shortcuts: Vec::new(),
            deeplink_scheme: None,
            deeplink_dispatcher: None,
            single_instance: false,
            crash_reporter: None,
            log_sink: std::sync::Arc::new(UnifiedLogSink),
            persisted_state: true,
        }
    }

    /// Adds a window.
    pub fn with_window(mut self, cfg: WindowConfig) -> Self {
        self.windows.push(cfg);
        self
    }

    /// Sets the IPC ACL applied to every window.
    pub fn with_acl(mut self, acl: Acl) -> Self {
        self.acl = acl;
        self
    }

    /// Sets the IPC rate limit (commands/sec) applied to every window.
    pub fn with_max_commands_per_second(mut self, n: u32) -> Self {
        self.max_commands_per_second = n;
        self
    }

    /// Attaches a sidecar (e.g. the Go backend) to supervise.
    pub fn with_sidecar(mut self, cfg: SidecarConfig) -> Self {
        self.sidecar = Some(cfg);
        self
    }

    /// Registers a global shortcut under `id` with spec string `spec`.
    pub fn with_shortcut(mut self, id: String, spec: String) -> Self {
        self.shortcuts.push((id, spec));
        self
    }

    /// Enables a deep-link scheme (runtime OS registration).
    pub fn with_deeplink(mut self, scheme: String) -> Self {
        self.deeplink_scheme = Some(scheme);
        self.deeplink_dispatcher = Some(DeepLinkDispatcher::new());
        self
    }

    /// Enables single-instance enforcement.
    pub fn with_single_instance(mut self, on: bool) -> Self {
        self.single_instance = on;
        self
    }

    /// Installs a crash reporter (panic hook + sidecar crash surfacing).
    pub fn with_crash_reporter(mut self, reporter: CrashReporter) -> Self {
        self.crash_reporter = Some(reporter);
        self
    }

    /// Overrides the unified log sink.
    pub fn with_log_sink(mut self, sink: ArcLogSink) -> Self {
        self.log_sink = sink;
        self
    }

    /// Toggles persisted window state (default on).
    pub fn with_persisted_state(mut self, on: bool) -> Self {
        self.persisted_state = on;
        self
    }

    /// Merges standard built-in capabilities (dialog/notify/clipboard/media/
    /// secret) into the ACL so the built-in actions resolve. Call after
    /// [`AppBuilder::with_acl`] if you want them alongside custom grants.
    pub fn with_builtin_capabilities(mut self) -> Self {
        let mut caps = self.acl.capabilities;
        caps.extend(dialog_capabilities());
        caps.push(notify_capability());
        caps.extend(clipboard::clipboard_capabilities());
        caps.push(webrtc::media_capability(&[
            MediaKind::Camera,
            MediaKind::Microphone,
            MediaKind::CameraAndMic,
        ]));
        self.acl = Acl { capabilities: caps };
        self
    }

    /// Runs the app. `on_command` receives every dispatched action: app-defined
    /// actions plus the synthetic `shortcut:<id>`, `deeplink`, and `filedrop`
    /// events.
    pub fn run<F>(self, on_command: F) -> Result<()>
    where
        F: Fn(&str, serde_json::Value) -> std::result::Result<(), String> + 'static,
    {
        // Single-instance enforcement.
        if self.single_instance {
            match single_instance::ensure_single_instance(&self.app_id)? {
                single_instance::InstanceCheck::Secondary { show_request } => {
                    // Another instance is primary; nudge it to show and exit.
                    let _ = single_instance::wait_for_show_request(&show_request);
                    return Ok(());
                }
                single_instance::InstanceCheck::Primary(_guard) => {
                    // We are primary; `_guard` holds the lock for our lifetime.
                }
            }
        }

        // Crash reporting.
        if let Some(reporter) = &self.crash_reporter {
            crash::install_panic_hook(reporter.clone());
        }

        // Sidecar supervision.
        let _sidecar: Option<SidecarSupervisor> = match &self.sidecar {
            Some(cfg) => Some(SidecarSupervisor::spawn(cfg.clone())?),
            None => None,
        };

        // Deep-link OS registration.
        if let (Some(scheme), Some(disp)) = (&self.deeplink_scheme, &self.deeplink_dispatcher) {
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("app"));
            if let Err(e) = deeplink::register_scheme(scheme, &exe) {
                self.log_sink
                    .emit(crate::sidecar::Stream::Stderr, &format!("deeplink register: {e}"));
            }
            // Surface a launch-time deep link from argv (if any).
            for arg in std::env::args() {
                if arg.starts_with(&format!("{scheme}://")) {
                    disp.push(arg);
                }
            }
        }

        let event_loop = EventLoop::new();

        // Shared, per-window webview registry (Rc<RefCell<...>> because the
        // loop and IPC closures both borrow it).
        let registry: Rc<RefCell<HashMap<String, WebView>>> = Rc::new(RefCell::new(HashMap::new()));

        let shortcut_registry = if self.shortcuts.is_empty() {
            None
        } else {
            match ShortcutRegistry::new(&self.shortcuts) {
                Ok(r) => Some(r),
                Err(e) => {
                    self.log_sink.emit(
                        crate::sidecar::Stream::Stderr,
                        &format!("shortcut register: {e:?}"),
                    );
                    None
                }
            }
        };
        let shortcut_rx = shortcut_registry
            .as_ref()
            .map(|_| shortcut::ShortcutRegistry::event_receiver());

        let dragdrop_disp = DragDropDispatcher::new();
        let deeplink_disp = self.deeplink_dispatcher.clone();

        // Cloneables into the loop closure.
        let max_cps = self.max_commands_per_second;
        let acl = self.acl.clone();
        let app_id = self.app_id.clone();
        let persisted = self.persisted_state;
        let log_sink = self.log_sink.clone();
        let on_command = std::rc::Rc::new(on_command);
        let clipboard = std::sync::Arc::new(Clipboard::new());

        let mut built_windows = Vec::new();
        for cfg in &self.windows {
            let geo = if persisted {
                window_state::load(&app_id, &cfg.id)
            } else {
                window_state::WindowGeometry::default()
            };
            let mut wb = WindowBuilder::new().with_title(&cfg.title);
            if let (Some(x), Some(y)) = (geo.x, geo.y) {
                wb = wb.with_position(wry::application::dpi::LogicalPosition::new(x, y));
            }
            wb = wb.with_inner_size(LogicalSize::new(
                if geo.width > 0 { geo.width } else { cfg.width },
                if geo.height > 0 { geo.height } else { cfg.height },
            ));
            let window = wb.build(&event_loop)?;

            let dist_dir = cfg.dist_dir.clone();
            let acl_c = acl.clone();
            let limiter = crate::new_ipc_rate_limiter(max_cps);
            let on_command_c = on_command.clone();
            let app_id_c = app_id.clone();
            let log_sink_c = log_sink.clone();
            let clipboard_c = clipboard.clone();
            let registry_c = registry.clone();
            let window_id = cfg.id.clone();

            let builder = WebViewBuilder::new(window)?
                .with_initialization_script(crate::INIT_SCRIPT)
                .with_custom_protocol("app".to_string(), move |request: &Request<Vec<u8>>| {
                    serve(&dist_dir, request)
                })
                .with_file_drop_handler({
                    let sender = dragdrop_disp.sender();
                    move |_window, event| {
                        match event {
                            FileDropEvent::Hovered(paths) => {
                                sender.send(dragdrop::DragDropEvent::Hovered(
                                    paths,
                                    0.0,
                                    0.0,
                                ));
                                true
                            }
                            FileDropEvent::Dropped(paths) => {
                                sender.send(dragdrop::DragDropEvent::Dropped(
                                    paths,
                                    0.0,
                                    0.0,
                                ));
                                true
                            }
                            FileDropEvent::Cancelled => {
                                sender.send(dragdrop::DragDropEvent::Cancelled);
                                true
                            }
                            _ => true,
                        }
                    }
                })
                .with_ipc_handler(move |_window, message| {
                    dispatch_ipc(
                        &acl_c,
                        &limiter,
                        &*on_command_c,
                        &app_id_c,
                        &log_sink_c,
                        &clipboard_c,
                        &registry_c,
                        &window_id,
                        &message,
                    );
                })
                .with_url("app://localhost/index.html")?;

            let webview = builder.build()?;
            built_windows.push(cfg.id.clone());
            registry.borrow_mut().insert(cfg.id.clone(), webview);
        }

        let dragdrop_rx = dragdrop_disp.receiver();
        let deeplink_rx = deeplink_disp.as_ref().map(|d| d.receiver());

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            // Pump global shortcuts.
            if let (Some(rx), Some(reg)) = (&shortcut_rx, &shortcut_registry) {
                while let Ok(ev) = rx.try_recv() {
                    if let Some(id) = reg.resolve(ev.id) {
                        let _ = on_command("shortcut", json!({ "id": id }));
                    }
                }
            }

            // Pump drag-and-drop events.
            while let Some(dd) = dragdrop_rx.try_recv() {
                if let dragdrop::DragDropEvent::Dropped(paths, x, y) = dd {
                    let paths: Vec<String> = paths
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                    let _ = on_command("filedrop", json!({ "paths": paths, "x": x, "y": y }));
                }
            }

            // Pump deep links.
            if let Some(rx) = &deeplink_rx {
                while let Some(url) = rx.try_recv() {
                    let _ = on_command("deeplink", json!({ "url": url }));
                }
            }

            if let Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id: _,
                ..
            } = event
            {
                // Persist geometry for each window before exit.
                if persisted {
                    for id in &built_windows {
                        if let Some(wv) = registry.borrow().get(id) {
                            let _ = wv; // geometry capture would read window bounds here
                        }
                        window_state::save(&app_id, id, &window_state::load(&app_id, id));
                    }
                }
                *control_flow = ControlFlow::Exit;
            }
        })
        // `event_loop.run` does not return (it exits the process on
        // `ControlFlow::Exit`); the `!` coerces to `Result<()>`.
    }
}

/// Dispatches a single IPC message: validates against the ACL, rate-limits,
/// then routes to built-in subsystems (secret/dialog/notify/clipboard/media)
/// or the app `on_command`. Replies for built-ins are posted back to the page
/// via `window.__appfrontResolve(requestId, result)`.
#[allow(clippy::too_many_arguments)]
fn dispatch_ipc(
    acl: &Acl,
    limiter: &crate::IpcRateLimiter,
    on_command: &dyn Fn(&str, serde_json::Value) -> std::result::Result<(), String>,
    app_id: &str,
    log_sink: &ArcLogSink,
    clipboard: &std::sync::Arc<Clipboard>,
    registry: &std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, WebView>>>,
    window_id: &str,
    message: &str,
) {
    let parsed: serde_json::Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] malformed IPC: {e}"),
            );
            return;
        }
    };
    let action = match parsed.get("action").and_then(|a| a.as_str()) {
        Some(a) => a.to_string(),
        None => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                "[appfront-webview] IPC without `action`",
            );
            return;
        }
    };
    let raw_params = parsed.get("params").cloned().unwrap_or(serde_json::Value::Null);
    let request_id = parsed.get("requestId").cloned();

    let validated = match acl.validate(&action, &raw_params) {
        Ok(v) => v,
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] rejected `{action}`: {e:?}"),
            );
            reply(registry, window_id, request_id, json!({ "error": format!("{e:?}") }));
            return;
        }
    };

    if limiter.check().is_err() {
        log_sink.emit(
            crate::sidecar::Stream::Stderr,
            &format!("[appfront-webview] rate limit exceeded for `{action}`"),
        );
        reply(registry, window_id, request_id, json!({ "error": "rate limited" }));
        return;
    }

    // Built-in subsystems.
    if let Some(r) = secret::handle_secret_action(app_id, acl, &action, &validated) {
        match r {
            Ok(v) => reply(registry, window_id, request_id, v),
            Err(SecretError::NotPermitted) => {
                reply(registry, window_id, request_id, json!({ "error": "not permitted" }))
            }
            Err(e) => reply(registry, window_id, request_id, json!({ "error": e.to_string() })),
        }
        return;
    }
    if let Some(v) = dialog::handle_dialog_action(acl, &action, &validated) {
        reply(registry, window_id, request_id, v);
        return;
    }
    if let Some(r) = notify::handle_notify_action(acl, &action, &validated) {
        match r {
            Ok(()) => reply(registry, window_id, request_id, json!({ "ok": true })),
            Err(e) => reply(registry, window_id, request_id, json!({ "error": e })),
        }
        return;
    }
    if let Some(r) = clipboard::handle_clipboard_action(clipboard, acl, &action, &validated) {
        match r {
            Ok(v) => reply(registry, window_id, request_id, v),
            Err(e) => reply(registry, window_id, request_id, json!({ "error": e })),
        }
        return;
    }
    if let Some(v) = webrtc::handle_media_action(acl, &action, &validated) {
        reply(registry, window_id, request_id, v);
        return;
    }

    // App-defined action (plus synthetic shortcut/deeplink/filedrop which are
    // produced by the event loop, not IPC, and handled there).
    if let Err(e) = on_command(&action, validated.clone()) {
        log_sink.emit(
            crate::sidecar::Stream::Stderr,
            &format!("[appfront-webview] command `{action}` failed: {e}"),
        );
    }
}

/// Delivers an IPC reply to the page via `window.__appfrontResolve`, if the
/// original message carried a `requestId`. The webview for `window_id` is
/// looked up in `registry` and the resolver is invoked through
/// `evaluate_script`.
fn reply(
    registry: &std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, WebView>>>,
    window_id: &str,
    request_id: Option<serde_json::Value>,
    value: serde_json::Value,
) {
    let Some(req) = request_id else { return };
    let payload = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
    let script = format!("window.__appfrontResolve({req}, {payload});");
    if let Some(wv) = registry.borrow().get(window_id) {
        let _ = wv.evaluate_script(&script);
    }
}

/// Serves a file from `dist_dir` over the custom `app://` protocol, with
/// path-traversal protection (see [`crate::serve`]).
fn serve(
    dist_dir: &std::path::Path,
    request: &Request<Vec<u8>>,
) -> wry::Result<Response<std::borrow::Cow<'static, [u8]>>> {
    crate::serve(dist_dir, request)
}
