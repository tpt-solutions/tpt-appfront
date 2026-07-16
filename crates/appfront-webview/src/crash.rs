//! Crash reporting hook: installs a Rust panic hook and surfaces sidecar
//! crash events, forwarding both to a telemetry callback.
//!
//! Titan wants a single place to learn about failures: native panics and the
//! Go backend dying. [`install_crash_hooks`] installs a panic hook that calls
//! the supplied reporter, and [`SidecarCrashMonitor`] wraps the
//! [`crate::SidecarSupervisor`] exit so unexpected backend deaths are reported
//! too.

use std::panic;

/// A reporter for crash events. `where_` is a short source label
/// (`"rust-panic"` / `"sidecar-crash"`), `detail` is the message.
pub type CrashReporter = std::sync::Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Installs a panic hook that forwards every panic to `reporter`. The
/// previously-installed hook (if any) is still invoked afterwards so default
/// behaviour (printing the backtrace) is preserved.
pub fn install_panic_hook(reporter: CrashReporter) {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        reporter("rust-panic", &format!("{msg} @ {location}"));
        prev(info);
    }));
}

/// A crash event captured from the sidecar.
#[derive(Debug, Clone)]
pub struct SidecarCrash {
    /// The backend program path.
    pub program: String,
    /// Detail about the crash (exit code or launch failure).
    pub detail: String,
}

/// Reports a sidecar crash to the configured [`CrashReporter`].
#[allow(dead_code)]
pub fn report_sidecar_crash(reporter: &CrashReporter, program: &str, detail: &str) {
    reporter("sidecar-crash", &format!("{program}: {detail}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn panic_hook_forwards_to_reporter() {
        let captured = Arc::new(Mutex::new(String::new()));
        let cap = captured.clone();
        install_panic_hook(Arc::new(move |source, detail| {
            *cap.lock().unwrap() = format!("{source}|{detail}");
        }));
        // Trigger a panic in a catch_unwind so the test doesn't abort.
        let result = panic::catch_unwind(|| panic!("boom"));
        assert!(result.is_err());
        let logged = captured.lock().unwrap().clone();
        assert!(logged.starts_with("rust-panic|"), "got: {logged}");
    }
}
