//! Native desktop backend for Windows and macOS: a `tao` event loop hosting
//! `wry` webviews. Builder types live in [`crate::app`]; the Linux backend is
//! `linux/` and is not compiled here.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use anyhow::Result;
use serde_json::json;
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Window, WindowBuilder};
use wry::http::{Request, Response};
use wry::{WebView, WebViewBuilder};

use crate::app::{AppBuilder, ArcLogSink, CommandHandler, CommandOutcome};
use crate::clipboard::{self, Clipboard};
use crate::dragdrop::{self, DragDropDispatcher};
use crate::secret::{self, SecretError};
use crate::shortcut::{self, ShortcutRegistry};
use crate::{dialog, notify, webrtc, window_state, Acl};

/// A native window and the webview hosted inside it. wry 0.57 does not own the
/// window, so the tao `Window` must be kept alive alongside its `WebView`.
struct ManagedWindow {
    // Declared before `window` so the webview is dropped first.
    webview: WebView,
    window: Window,
}

/// Per-window registry, shared between the event loop and IPC closures.
type WindowRegistry = Rc<RefCell<HashMap<String, ManagedWindow>>>;
impl AppBuilder {
    /// Runs the native event loop. Called by `AppBuilder::run_inner` after the
    /// platform-neutral startup work.
    pub(crate) fn run_native(self, on_command: Rc<CommandHandler>) -> Result<()> {
        let event_loop = EventLoop::new();

        // Shared, per-window webview registry (Rc<RefCell<...>> because the
        // loop and IPC closures both borrow it).
        let registry: WindowRegistry = Rc::new(RefCell::new(HashMap::new()));

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
        let on_command = on_command; // already an Rc<CommandHandler>
        let clipboard = std::sync::Arc::new(Clipboard::new());
        let on_reject = self.on_ipc_rejection.clone();

        let mut built_windows = Vec::new();
        for cfg in &self.windows {
            let geo = if persisted {
                window_state::load(&app_id, &cfg.id)
            } else {
                window_state::WindowGeometry::default()
            };
            let mut wb = WindowBuilder::new().with_title(&cfg.title);
            if let (Some(x), Some(y)) = (geo.x, geo.y) {
                wb = wb.with_position(LogicalPosition::new(x, y));
            }
            wb = wb.with_inner_size(LogicalSize::new(
                if geo.width > 0 { geo.width } else { cfg.width },
                if geo.height > 0 {
                    geo.height
                } else {
                    cfg.height
                },
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
            let on_reject_c = on_reject.clone();
            let window_id = cfg.id.clone();

            let webview = WebViewBuilder::new()
                .with_initialization_script(crate::INIT_SCRIPT)
                .with_custom_protocol("app".to_string(), move |_id, request| {
                    serve(&dist_dir, &request)
                })
                .with_drag_drop_handler({
                    let sender = dragdrop_disp.sender();
                    move |event| {
                        match event {
                            wry::DragDropEvent::Enter { paths, position } => {
                                sender.send(dragdrop::DragDropEvent::Hovered(
                                    paths,
                                    f64::from(position.0),
                                    f64::from(position.1),
                                ));
                            }
                            wry::DragDropEvent::Drop { paths, position } => {
                                sender.send(dragdrop::DragDropEvent::Dropped(
                                    paths,
                                    f64::from(position.0),
                                    f64::from(position.1),
                                ));
                            }
                            wry::DragDropEvent::Leave => {
                                sender.send(dragdrop::DragDropEvent::Cancelled);
                            }
                            // `Over` carries no paths; the hover was already reported on `Enter`.
                            _ => {}
                        }
                        true
                    }
                })
                .with_ipc_handler(move |request: Request<String>| {
                    dispatch_ipc(
                        &acl_c,
                        &limiter,
                        &*on_command_c,
                        &app_id_c,
                        &log_sink_c,
                        &clipboard_c,
                        &registry_c,
                        &window_id,
                        &on_reject_c,
                        request.body(),
                    );
                })
                .with_url("app://localhost/index.html")
                .build(&window)?;

            built_windows.push(cfg.id.clone());
            registry
                .borrow_mut()
                .insert(cfg.id.clone(), ManagedWindow { webview, window });
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
                ..
            } = event
            {
                // Persist geometry for each window before exit, reading the
                // live window bounds rather than re-saving what was loaded.
                if persisted {
                    for id in &built_windows {
                        let geo = match registry.borrow().get(id) {
                            Some(mw) => {
                                let window = &mw.window;
                                let scale = window.scale_factor();
                                let pos = window.outer_position().ok();
                                let size = window.inner_size().to_logical::<u32>(scale);
                                window_state::WindowGeometry {
                                    x: pos.as_ref().map(|p| p.x),
                                    y: pos.as_ref().map(|p| p.y),
                                    width: size.width,
                                    height: size.height,
                                }
                            }
                            None => window_state::load(&app_id, id),
                        };
                        window_state::save(&app_id, id, &geo);
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
    on_command: &CommandHandler,
    app_id: &str,
    log_sink: &ArcLogSink,
    clipboard: &std::sync::Arc<Clipboard>,
    registry: &WindowRegistry,
    window_id: &str,
    on_reject: &Option<std::sync::Arc<dyn Fn(crate::IpcRejection) + Send + Sync>>,
    message: &str,
) {
    // Bound the message before parsing so a hosted page cannot make the native process parse an
    // arbitrarily large payload.
    if message.len() > crate::MAX_IPC_MESSAGE_BYTES {
        log_sink.emit(
            crate::sidecar::Stream::Stderr,
            &format!(
                "[appfront-webview] rejecting IPC message: {} bytes exceeds {} byte limit",
                message.len(),
                crate::MAX_IPC_MESSAGE_BYTES
            ),
        );
        if let Some(h) = on_reject {
            h(crate::IpcRejection::TooLarge {
                bytes: message.len(),
            });
        }
        return;
    }
    let parsed: serde_json::Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] malformed IPC: {e}"),
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::Malformed {
                    error: e.to_string(),
                });
            }
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
            if let Some(h) = on_reject {
                h(crate::IpcRejection::MissingAction);
            }
            return;
        }
    };
    let raw_params = parsed
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let request_id = parsed.get("requestId").cloned();

    let validated = match acl.validate(&action, &raw_params) {
        Ok(v) => v,
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] rejected `{action}`: {e:?}"),
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::ActionDenied {
                    action,
                    reason: format!("{e:?}"),
                });
            }
            reply(
                registry,
                window_id,
                request_id,
                json!({ "error": format!("{e:?}") }),
            );
            return;
        }
    };

    if limiter.check().is_err() {
        log_sink.emit(
            crate::sidecar::Stream::Stderr,
            &format!("[appfront-webview] rate limit exceeded for `{action}`"),
        );
        if let Some(h) = on_reject {
            h(crate::IpcRejection::RateLimited { action });
        }
        reply(
            registry,
            window_id,
            request_id,
            json!({ "error": "rate limited" }),
        );
        return;
    }

    // Built-in subsystems.
    if let Some(r) = secret::handle_secret_action(app_id, acl, &action, &validated) {
        match r {
            Ok(v) => reply(registry, window_id, request_id, v),
            Err(SecretError::NotPermitted) => reply(
                registry,
                window_id,
                request_id,
                json!({ "error": "not permitted" }),
            ),
            Err(e) => reply(
                registry,
                window_id,
                request_id,
                json!({ "error": e.to_string() }),
            ),
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
    match on_command(&action, validated.clone()) {
        Ok(CommandOutcome::Reply(value)) => {
            reply(registry, window_id, request_id, value);
        }
        Ok(CommandOutcome::NoReply) => {}
        Err(e) => {
            log_sink.emit(
                crate::sidecar::Stream::Stderr,
                &format!("[appfront-webview] command `{action}` failed: {e}"),
            );
            if let Some(h) = on_reject {
                h(crate::IpcRejection::DispatchError { action, error: e });
            }
        }
    }
}

/// Delivers an IPC reply to the page via `window.__appfrontResolve`, if the
/// original message carried a `requestId`. The webview for `window_id` is
/// looked up in `registry` and the resolver is invoked through
/// `evaluate_script`.
fn reply(
    registry: &WindowRegistry,
    window_id: &str,
    request_id: Option<serde_json::Value>,
    value: serde_json::Value,
) {
    let Some(req) = request_id else { return };
    let payload = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
    let script = format!("window.__appfrontResolve({req}, {payload});");
    if let Some(mw) = registry.borrow().get(window_id) {
        let _ = mw.webview.evaluate_script(&script);
    }
}

/// Serves a file from `dist_dir` over the custom `app://` protocol, with
/// path-traversal protection (see [`crate::serve`]). Internal failures are
/// answered with a 500, since wry's protocol handler returns a plain response.
fn serve(dist_dir: &std::path::Path, request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    crate::serve(dist_dir, request).unwrap_or_else(|_| {
        Response::builder()
            .status(500)
            .body(Cow::Borrowed(b"500 Internal Server Error" as &[u8]))
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"500" as &[u8])))
    })
}
