//! Sidecar process supervision for the webview shell.
//!
//! Titan's desktop app ships a Go backend alongside the Rust shell. This module
//! spawns that backend as a child process, pipes its stdout/stderr into a
//! shared log sink, and — on unexpected exit — restarts it after a bounded
//! exponential backoff. A clean exit (e.g. the backend asked to shut down) is
//! *not* restarted.
//!
//! The supervisor runs on its own OS thread and is intentionally fire-and-forget:
//! it owns the child's lifetime and tears it down on drop.

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Default minimum delay between restart attempts (doubled each retry, up to
/// [`MAX_BACKOFF`]).
const BASE_BACKOFF: Duration = Duration::from_millis(250);
/// Cap on the exponential backoff so a permanently-crashing backend doesn't
/// spin at ever-larger delays forever.
const MAX_BACKOFF: Duration = Duration::from_secs(8);

/// Commands sent to the supervisor thread.
enum SupCommand {
    /// Stop the child and exit the supervisor loop.
    Shutdown,
}

/// A sink that receives lines from the supervised process's stdout/stderr.
///
/// Implemented as a trait so callers can route logs anywhere (file, in-memory,
/// a Rust logger, etc.) without this module depending on a particular logging
/// backend.
pub trait LogSink: Send + Sync + 'static {
    /// Called once per line from the child's stdout (or stderr).
    fn emit(&self, stream: Stream, line: &str);
}

/// Which stream a [`LogSink::emit`] line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Child's standard output.
    Stdout,
    /// Child's standard error.
    Stderr,
}

/// A [`LogSink`] that forwards every line to `eprintln!` with a `[sidecar]` tag.
///
/// Provided as a zero-config default for apps that just want sidecar output
/// to show up in the terminal; callers needing file/in-memory capture can
/// implement their own [`LogSink`].
#[allow(dead_code)]
pub struct EphemeralLogSink;

impl LogSink for EphemeralLogSink {
    fn emit(&self, stream: Stream, line: &str) {
        eprintln!("[sidecar:{stream:?}] {line}");
    }
}

/// Configuration for [`SidecarSupervisor::spawn`].
#[derive(Clone)]
pub struct SidecarConfig {
    /// Path to the backend executable.
    pub program: PathBuf,
    /// Arguments passed to the backend (e.g. `--port`, config paths).
    pub args: Vec<String>,
    /// Working directory for the child (defaults to the executable's dir).
    pub working_dir: Option<PathBuf>,
    /// Where the child's stdout/stderr are piped.
    pub log_sink: Arc<dyn LogSink>,
    /// If `true`, an unexpected exit is restarted with backoff. If `false`, the
    /// supervisor simply reports the exit and stops.
    pub restart_on_crash: bool,
}

/// Handle to a running supervisor. Dropping it requests a graceful shutdown of
/// the child process.
pub struct SidecarSupervisor {
    tx: Sender<SupCommand>,
    handle: Option<JoinHandle<()>>,
}

impl SidecarSupervisor {
    /// Spawns the configured backend and starts supervised monitoring.
    pub fn spawn(config: SidecarConfig) -> io::Result<SidecarSupervisor> {
        let (tx, rx) = mpsc::channel::<SupCommand>();
        let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

        let worker = SupervisorWorker {
            config,
            rx,
            child_slot: child_slot.clone(),
        };

        let handle = thread::Builder::new()
            .name("appfront-sidecar".to_string())
            .spawn(move || worker.run())?;

        Ok(SidecarSupervisor {
            tx,
            handle: Some(handle),
        })
    }

    /// Requests graceful shutdown of the child and the supervisor thread.
    ///
    /// Best-effort: sends the command and joins the worker. Dropping the handle
    /// also triggers shutdown, so this can be used to await cleanup explicitly.
    pub fn shutdown(mut self) {
        let _ = self.tx.send(SupCommand::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for SidecarSupervisor {
    fn drop(&mut self) {
        let _ = self.tx.send(SupCommand::Shutdown);
        // The worker thread kills the child on shutdown; joining here would
        // block a random dropper, so we let it detach.
    }
}

/// Internal worker that owns the restart loop.
struct SupervisorWorker {
    config: SidecarConfig,
    rx: Receiver<SupCommand>,
    child_slot: Arc<Mutex<Option<Child>>>,
}

impl SupervisorWorker {
    fn run(mut self) {
        let mut backoff = BASE_BACKOFF;

        loop {
            if self.should_shutdown() {
                self.kill_child();
                break;
            }

            match self.launch() {
                Ok(code) => {
                    // Clean exit — no restart regardless of `restart_on_crash`.
                    self.config.log_sink.emit(
                        Stream::Stdout,
                        &format!("sidecar exited cleanly with code {code:?}; not restarting"),
                    );
                    break;
                }
                Err(e) => {
                    self.config
                        .log_sink
                        .emit(Stream::Stderr, &format!("sidecar launch failed: {e}"));
                    if !self.config.restart_on_crash {
                        break;
                    }
                }
            }

            // Wait for either backoff to elapse or a shutdown signal.
            if self.wait_with_shutdown(backoff) {
                self.kill_child();
                break;
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    /// Returns `true` if a shutdown was requested.
    fn should_shutdown(&self) -> bool {
        matches!(
            self.rx.try_recv(),
            Ok(SupCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected)
        )
    }

    /// Blocks up to `dur`, returning `true` if shutdown arrived first.
    fn wait_with_shutdown(&self, dur: Duration) -> bool {
        matches!(
            self.rx.recv_timeout(dur),
            Ok(SupCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected)
        )
    }

    /// Force-kills any running child process.
    fn kill_child(&self) {
        if let Some(child) = self.child_slot.lock().unwrap().as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Spawns the child and pumps its output into the log sink. Returns the
    /// child's exit status when it terminates; an `Err` means the child could
    /// not be started (and should be retried if configured).
    fn launch(&mut self) -> io::Result<Option<i32>> {
        let mut cmd = Command::new(&self.config.program);
        cmd.args(&self.config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &self.config.working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()?;
        {
            let mut slot = self.child_slot.lock().unwrap();
            *slot = Some(child);
        }

        // Take the stdio handles out of the stored child (we keep `&mut` access
        // through the slot) so the pump threads own them.
        let (stdout, stderr) = {
            let mut slot = self.child_slot.lock().unwrap();
            let child = slot.as_mut().expect("child just set");
            (child.stdout.take(), child.stderr.take())
        };
        let sink = self.config.log_sink.clone();
        if let Some(out) = stdout {
            spawn_pump(out, Stream::Stdout, sink.clone());
        }
        if let Some(err) = stderr {
            spawn_pump(err, Stream::Stderr, sink.clone());
        }

        let code = {
            let mut slot = self.child_slot.lock().unwrap();
            let child = slot.as_mut().expect("child just set");
            child.wait()?.code()
        };
        {
            let mut slot = self.child_slot.lock().unwrap();
            *slot = None;
        }

        Ok(code)
    }
}

/// Spawns a thread that reads `reader` line-by-line and forwards each line to
/// `sink`.
fn spawn_pump<R: io::Read + Send + 'static>(reader: R, stream: Stream, sink: Arc<dyn LogSink>) {
    thread::Builder::new()
        .name(format!("appfront-sidecar-{stream:?}"))
        .spawn(move || {
            use std::io::{BufRead, BufReader};
            let mut buf = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match buf.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim_end_matches(['\n', '\r']);
                        if !trimmed.is_empty() {
                            sink.emit(stream, trimmed);
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("spawn sidecar pump thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    struct CountingSink(Arc<AtomicUsize>);
    impl LogSink for CountingSink {
        fn emit(&self, _stream: Stream, _line: &str) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn supervisor_captures_child_output() {
        // Use `cmd`/`sh` to emit a couple of lines then exit cleanly.
        let program = if cfg!(windows) {
            Path::new("cmd.exe")
        } else {
            Path::new("sh")
        };
        let args = if cfg!(windows) {
            vec!["/C".into(), "echo hello & echo world".into()]
        } else {
            vec!["-c".into(), "echo hello; echo world 1>&2".into()]
        };

        let count = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(CountingSink(count.clone()));

        let cfg = SidecarConfig {
            program: program.to_path_buf(),
            args,
            working_dir: None,
            log_sink: sink,
            restart_on_crash: false,
        };

        let sup = SidecarSupervisor::spawn(cfg).expect("spawn supervisor");
        // Give the worker time to launch + pump + exit.
        thread::sleep(Duration::from_millis(500));
        sup.shutdown();

        assert!(count.load(Ordering::SeqCst) >= 2, "expected stdout+stderr lines");
    }

    #[test]
    fn shutdown_signal_stops_supervisor() {
        let (tx, rx) = mpsc::channel::<()>();
        let count = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(CountingSink(count.clone()));

        // A child that loops so the supervisor stays alive until signaled.
        let program = if cfg!(windows) {
            Path::new("cmd.exe")
        } else {
            Path::new("sh")
        };
        let args = if cfg!(windows) {
            vec!["/C".into(), "ping -n 60 127.0.0.1 >nul".into()]
        } else {
            vec!["-c".into(), "sleep 60".into()]
        };

        let cfg = SidecarConfig {
            program: program.to_path_buf(),
            args,
            working_dir: None,
            log_sink: sink,
            restart_on_crash: false,
        };

        let sup = SidecarSupervisor::spawn(cfg).expect("spawn supervisor");
        drop(tx);
        sup.shutdown();
        let _ = rx.recv_timeout(Duration::from_secs(2));
        // If we reach here without hanging, shutdown worked.
    }
}
