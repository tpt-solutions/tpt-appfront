//! Native desktop backend for Windows and macOS: a `tpt-window` event loop hosting
//! `wry` webviews. Builder types live in [`crate::app`]; the Linux backend is
//! `linux/` and is not compiled here.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;
use tpt_window::{
    new_event_loop, EventQueue, EventSink, LogicalPosition, LogicalSize, PollStatus, SurfaceTarget,
    Window, WindowBuilder, WindowEvent, DEFAULT_CAPACITY,
};
use wry::http::{Request, Response};
use wry::{WebView, WebViewBuilder};

use crate::app::{AppBuilder, CommandHandler};
use crate::clipboard::Clipboard;
use crate::dragdrop::{self, DragDropDispatcher};
use crate::ipc_dispatch::dispatch_ipc;
use crate::shortcut::ShortcutRegistry;
use crate::window_state;

/// A native window and the webview hosted inside it. wry 0.57 does not own the
/// window, so the `tpt_window::Window` must be kept alive alongside its `WebView`.
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
        let mut event_loop = new_event_loop()?;

        // Shared, per-window webview registry (Rc<RefCell<...>> because the
        // loop and IPC closures both borrow it).
        let registry: WindowRegistry = Rc::new(RefCell::new(HashMap::new()));

        // Register the configured global shortcuts. A failure is logged, and the app runs
        // without them. The registry must stay alive for as long as the loop runs.
        let mut shortcut_registry = if self.shortcuts.is_empty() {
            None
        } else {
            match ShortcutRegistry::new(&self.shortcuts) {
                Ok(registry) => Some(registry),
                Err(e) => {
                    self.log_sink.emit(
                        crate::sidecar::Stream::Stderr,
                        &format!("{e}; {} shortcut(s) ignored", self.shortcuts.len()),
                    );
                    None
                }
            }
        };

        // The tray icon, if the app asked for one. Like shortcuts, a failure is logged and the
        // app runs without it.
        #[cfg(feature = "tray")]
        let mut tray = match self.tray.clone() {
            None => None,
            Some((tooltip, items)) => match crate::tray::TrayController::new(&tooltip) {
                Ok(mut controller) => {
                    controller.set_menu(&items);
                    Some(controller)
                }
                Err(e) => {
                    self.log_sink
                        .emit(crate::sidecar::Stream::Stderr, &format!("tray: {e}"));
                    None
                }
            },
        };

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
                wb = wb.with_position(LogicalPosition::new(f64::from(x), f64::from(y)));
            }
            let width = if geo.width > 0 { geo.width } else { cfg.width };
            let height = if geo.height > 0 {
                geo.height
            } else {
                cfg.height
            };
            wb = wb.with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)));
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
                    let eval = |script: &str| {
                        if let Some(mw) = registry_c.borrow().get(&window_id) {
                            let _ = mw.webview.evaluate_script(script);
                        }
                    };
                    dispatch_ipc(
                        &acl_c,
                        &limiter,
                        &*on_command_c,
                        &app_id_c,
                        &log_sink_c,
                        &clipboard_c,
                        &eval,
                        &on_reject_c,
                        request.body(),
                    );
                })
                .with_url("app://localhost/index.html")
                .build(&SurfaceTarget::new(&window))?;

            built_windows.push(cfg.id.clone());
            registry
                .borrow_mut()
                .insert(cfg.id.clone(), ManagedWindow { webview, window });
        }

        let dragdrop_rx = dragdrop_disp.receiver();
        let deeplink_rx = deeplink_disp.as_ref().map(|d| d.receiver());

        let mut queue = EventQueue::with_capacity(DEFAULT_CAPACITY);
        'run: loop {
            // Hotkey presses and tray clicks arrive as window messages, which do not wake a
            // blocking wait on their own. While shortcuts or a tray are active, wait at most
            // 50 ms (20 Hz). Otherwise wait at most 250 ms, so deep links and drag-and-drop
            // from the channels below are picked up without an OS event.
            #[cfg(feature = "tray")]
            let tray_active = tray.is_some();
            #[cfg(not(feature = "tray"))]
            let tray_active = false;
            let wait = if shortcut_registry.is_some() || tray_active {
                Duration::from_millis(50)
            } else {
                Duration::from_millis(250)
            };
            let status = event_loop.step(&mut EventSink::new(&mut queue), wait);

            // Pump global shortcuts.
            if let Some(registry) = shortcut_registry.as_mut() {
                for id in registry.take_pressed() {
                    let _ = on_command("shortcut", json!({ "id": id }));
                }
            }

            // Pump tray events: menu presses and left clicks.
            #[cfg(feature = "tray")]
            if let Some(tray) = tray.as_mut() {
                while let Some(event) = tray.next_event() {
                    let _ = match event {
                        crate::tray::TrayEvent::MenuItem(id) => {
                            on_command("tray.menu", json!({ "id": id }))
                        }
                        crate::tray::TrayEvent::Activate => on_command("tray.activate", json!({})),
                    };
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

            let mut close_requested = false;
            for ev in queue.drain() {
                if ev.event == WindowEvent::CloseRequested {
                    close_requested = true;
                }
            }

            if close_requested {
                // Persist geometry for each window before exit, reading the
                // live window bounds rather than re-saving what was loaded.
                if persisted {
                    for id in &built_windows {
                        let geo = match registry.borrow().get(id) {
                            Some(mw) => {
                                let window = &mw.window;
                                let pos = window.position();
                                let size = window.logical_size();
                                window_state::WindowGeometry {
                                    x: Some(pos.x),
                                    y: Some(pos.y),
                                    width: size.width.round() as u32,
                                    height: size.height.round() as u32,
                                }
                            }
                            None => window_state::load(&app_id, id),
                        };
                        window_state::save(&app_id, id, &geo);
                    }
                }
                break 'run;
            }

            if status == PollStatus::Exit {
                break 'run;
            }
        }
        Ok(())
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
