//! Platform-neutral application builder: windows, ACL, sidecar, shortcuts,
//! deep links, and lifecycle options, plus the command-handler types. The
//! native event loop lives in the per-platform backend (`manager.rs` on
//! Windows/macOS, `linux/` on Linux). See `docs/linux-webview-backend.md`.

use std::path::PathBuf;

use anyhow::Result;

use crate::crash::{self, CrashReporter};
use crate::deeplink::{self, DeepLinkDispatcher};
use crate::dialog::dialog_capabilities;
use crate::logging::UnifiedLogSink;
use crate::notify::notify_capability;
use crate::sidecar::{LogSink, SidecarConfig, SidecarSupervisor};
use crate::webrtc::{self, MediaKind};
use crate::{clipboard, single_instance, Acl, WebviewOptions};

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

/// What an app command produced.
///
/// `NoReply` is the legacy fire-and-forget contract (`AppBuilder::run`):
/// nothing is ever delivered to the page, even if it passed a `requestId`.
/// `Reply` carries a JSON payload delivered via
/// `window.__appfrontResolve(requestId, result)` when the page asked for one
/// (`AppBuilder::run_with_value_handler`).
pub enum CommandOutcome {
    NoReply,
    Reply(serde_json::Value),
}

/// Type-erased app command handler as dispatch sees it.
pub type CommandHandler =
    dyn Fn(&str, serde_json::Value) -> std::result::Result<CommandOutcome, String>;

/// The application builder. Accumulates windows, sidecar, shortcuts, deep-link
/// scheme, and lifecycle options, then [`AppBuilder::run`]s the event loop.
pub struct AppBuilder {
    pub(crate) app_id: String,
    pub(crate) windows: Vec<WindowConfig>,
    pub(crate) acl: Acl,
    pub(crate) max_commands_per_second: u32,
    pub(crate) sidecar: Option<SidecarConfig>,
    pub(crate) shortcuts: Vec<(String, String)>,
    pub(crate) deeplink_scheme: Option<String>,
    pub(crate) deeplink_dispatcher: Option<DeepLinkDispatcher>,
    pub(crate) single_instance: bool,
    pub(crate) crash_reporter: Option<CrashReporter>,
    pub(crate) log_sink: ArcLogSink,
    pub(crate) persisted_state: bool,
    pub(crate) on_ipc_rejection: Option<std::sync::Arc<dyn Fn(crate::IpcRejection) + Send + Sync>>,
}

pub(crate) type ArcLogSink = std::sync::Arc<dyn LogSink>;
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
            on_ipc_rejection: None,
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

    /// Sets the host-side rejection hook invoked whenever an IPC message is
    /// rejected or fails (oversized, malformed, not grantable by the ACL,
    /// rate-limited, or the app `on_command` errors). Mirrors
    /// [`crate::WebviewOptions::on_ipc_rejection`]; defaults to `None`.
    pub fn with_ipc_rejection(
        mut self,
        hook: std::sync::Arc<dyn Fn(crate::IpcRejection) + Send + Sync>,
    ) -> Self {
        self.on_ipc_rejection = Some(hook);
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
    /// Runs the event loop with a fire-and-forget command handler: command
    /// results are never delivered back to the page, even when it passes a
    /// `requestId`. Use [`AppBuilder::run_with_value_handler`] when the page
    /// needs command results.
    pub fn run<F>(self, on_command: F) -> Result<()>
    where
        F: Fn(&str, serde_json::Value) -> std::result::Result<(), String> + 'static,
    {
        self.run_inner(std::rc::Rc::new(
            move |action: &str, params: serde_json::Value| {
                on_command(action, params).map(|_| CommandOutcome::NoReply)
            },
        ))
    }

    /// Runs the event loop with a value-returning command handler: every
    /// `Ok` payload is delivered to the page via
    /// `window.__appfrontResolve(requestId, result)` when the IPC message
    /// carried a `requestId`, and `Err` is logged + surfaced through the
    /// rejection hook as before. Commands run on the event-loop thread, so
    /// long-running work belongs on a thread the handler joins with a
    /// timeout.
    pub fn run_with_value_handler<F>(self, on_command: F) -> Result<()>
    where
        F: Fn(&str, serde_json::Value) -> std::result::Result<serde_json::Value, String> + 'static,
    {
        self.run_inner(std::rc::Rc::new(
            move |action: &str, params: serde_json::Value| {
                on_command(action, params).map(CommandOutcome::Reply)
            },
        ))
    }
    fn run_inner(self, on_command: std::rc::Rc<CommandHandler>) -> Result<()> {
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
                self.log_sink.emit(
                    crate::sidecar::Stream::Stderr,
                    &format!("deeplink register: {e}"),
                );
            }
            // Surface a launch-time deep link from argv (if any).
            for arg in std::env::args() {
                if arg.starts_with(&format!("{scheme}://")) {
                    disp.push(arg);
                }
            }
        }

        self.run_native(on_command)
    }
}
