//! Unified logging for the shell: routes both sidecar stdout/stderr and native
//! Rust shell log lines through a single [`LogSink`] so they interleave in one
//! place (terminal, file, or whatever the app configures).

use crate::sidecar::{LogSink, Stream};
use std::sync::Arc;

/// Convenience macro that forwards a native shell log line through the unified
/// sink as [`Stream::Stdout`].
#[macro_export]
macro_rules! shell_log {
    ($sink:expr, $($arg:tt)*) => {{
        ($sink).emit($crate::sidecar::Stream::Stdout, &format!($($arg)*));
    }};
}

/// A [`LogSink`] that writes every line (sidecar or shell) to `stdout` with a
/// stream tag. This is the unified default: sidecar output and shell log lines
/// no longer go to two different places.
#[derive(Debug, Default, Clone)]
pub struct UnifiedLogSink;

impl LogSink for UnifiedLogSink {
    fn emit(&self, stream: Stream, line: &str) {
        match stream {
            Stream::Stdout => println!("[appfront:{stream:?}] {line}"),
            Stream::Stderr => eprintln!("[appfront:{stream:?}] {line}"),
        }
    }
}

/// Tag a line as originating from the native shell (not the sidecar) and push it
/// through the same sink the sidecar uses, keeping a single log stream.
#[allow(dead_code)]
pub fn log_shell(sink: &Arc<dyn LogSink>, line: &str) {
    sink.emit(Stream::Stdout, line);
}

/// Tag a native shell *error* line and push it through the unified sink.
#[allow(dead_code)]
pub fn log_shell_error(sink: &Arc<dyn LogSink>, line: &str) {
    sink.emit(Stream::Stderr, line);
}
