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

use crate::crash::CrashReporter;
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
    /// Called with `("sidecar-crash", detail)` for every unexpected exit (a
    /// non-zero status, a signal, or a failed launch), whether or not the
    /// child is restarted. `None` disables the report.
    pub crash_reporter: Option<CrashReporter>,
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
            stop: false,
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

/// How often the worker checks on a running child while waiting for it to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Internal worker that owns the restart loop.
struct SupervisorWorker {
    config: SidecarConfig,
    rx: Receiver<SupCommand>,
    child_slot: Arc<Mutex<Option<Child>>>,
    /// Set once a shutdown has been requested. It is sticky, because the
    /// channel message is consumed the first time it is seen.
    stop: bool,
}

impl SupervisorWorker {
    fn run(mut self) {
        let mut backoff = BASE_BACKOFF;

        loop {
            if self.poll_shutdown() {
                self.kill_child();
                break;
            }

            let outcome = self.launch();
            if self.stop {
                // Shut down while the child was running; `launch` has already killed it.
                break;
            }
            match outcome {
                Ok(Some(0)) => {
                    // Clean exit — no restart regardless of `restart_on_crash`.
                    self.config.log_sink.emit(
                        Stream::Stdout,
                        "sidecar exited cleanly with code 0; not restarting",
                    );
                    break;
                }
                Ok(code) => {
                    let status = code.map_or_else(
                        || "no exit code (terminated by a signal)".to_string(),
                        |c| format!("exit code {c}"),
                    );
                    self.report_crash(&format!("exited with {status}"));
                    if !self.config.restart_on_crash {
                        break;
                    }
                }
                Err(e) => {
                    self.config
                        .log_sink
                        .emit(Stream::Stderr, &format!("sidecar launch failed: {e}"));
                    self.report_crash(&format!("launch failed: {e}"));
                    if !self.config.restart_on_crash {
                        break;
                    }
                }
            }

            // Wait for either backoff to elapse or a shutdown signal.
            if self.wait_with_shutdown(backoff) {
                break;
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    /// Sends an unexpected exit to the configured crash reporter, if any.
    fn report_crash(&self, detail: &str) {
        if let Some(reporter) = &self.config.crash_reporter {
            reporter(
                "sidecar-crash",
                &format!("{}: {detail}", self.config.program.display()),
            );
        }
    }

    /// Records a pending shutdown (explicit or because the handle was dropped). Returns the sticky
    /// flag, so callers can ask again later.
    fn poll_shutdown(&mut self) -> bool {
        if !self.stop {
            self.stop = matches!(
                self.rx.try_recv(),
                Ok(SupCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected)
            );
        }
        self.stop
    }

    /// Blocks up to `dur`, returning `true` if shutdown arrived first.
    fn wait_with_shutdown(&mut self, dur: Duration) -> bool {
        if !self.stop {
            self.stop = matches!(
                self.rx.recv_timeout(dur),
                Ok(SupCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected)
            );
        }
        self.stop
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

        // Poll rather than block in `wait`, so a shutdown can stop a child that is still running.
        let exit = loop {
            let polled = {
                let mut slot = self.child_slot.lock().unwrap();
                let child = slot.as_mut().expect("child just set");
                child.try_wait()?
            };
            if let Some(status) = polled {
                break status.code();
            }
            if self.poll_shutdown() {
                self.kill_child();
                self.child_slot.lock().unwrap().take();
                // The caller sees `stop` and does not read this value.
                return Ok(None);
            }
            thread::sleep(POLL_INTERVAL);
        };
        self.child_slot.lock().unwrap().take();

        Ok(exit)
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
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use super::*;

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
            crash_reporter: None,
        };

        let sup = SidecarSupervisor::spawn(cfg).expect("spawn supervisor");
        // Wait for both lines rather than a fixed time: a shutdown stops a child that is still
        // running, so an early shutdown could cut the output short.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while count.load(Ordering::SeqCst) < 2 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        sup.shutdown();

        assert!(
            count.load(Ordering::SeqCst) >= 2,
            "expected stdout+stderr lines"
        );
    }

    #[test]
    fn nonzero_exit_is_reported_and_restarted() {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd.exe", vec!["/C".into(), "exit 3".into()])
        } else {
            ("sh", vec!["-c".into(), "exit 3".into()])
        };
        let reports = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink_reports = reports.clone();
        let reporter: CrashReporter = Arc::new(move |source: &str, detail: &str| {
            assert_eq!(source, "sidecar-crash");
            sink_reports.lock().unwrap().push(detail.to_string());
        });
        let cfg = SidecarConfig {
            program: PathBuf::from(program),
            args,
            working_dir: None,
            log_sink: Arc::new(CountingSink(Arc::new(AtomicUsize::new(0)))),
            restart_on_crash: true,
            crash_reporter: Some(reporter),
        };

        let sup = SidecarSupervisor::spawn(cfg).expect("spawn supervisor");
        // Backoff starts at 250 ms. Process start-up can be slow on Windows, so wait for the
        // restart to show up rather than sleeping a fixed time.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while reports.lock().unwrap().len() < 2 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        sup.shutdown();

        let reports = reports.lock().unwrap();
        assert!(reports.len() >= 2, "expected a restart, got {reports:?}");
        assert!(reports[0].contains("exit code 3"), "{reports:?}");
    }

    #[test]
    fn shutdown_stops_a_running_child_promptly() {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd.exe", vec!["/C".into(), "ping -n 60 127.0.0.1 >nul".into()])
        } else {
            ("sh", vec!["-c".into(), "sleep 60".into()])
        };
        let reports = Arc::new(Mutex::new(0usize));
        let sink_reports = reports.clone();
        let reporter: CrashReporter = Arc::new(move |_: &str, _: &str| {
            *sink_reports.lock().unwrap() += 1;
        });
        let cfg = SidecarConfig {
            program: PathBuf::from(program),
            args,
            working_dir: None,
            log_sink: Arc::new(CountingSink(Arc::new(AtomicUsize::new(0)))),
            restart_on_crash: true,
            crash_reporter: Some(reporter),
        };

        let sup = SidecarSupervisor::spawn(cfg).expect("spawn supervisor");
        thread::sleep(Duration::from_millis(200));
        let started = std::time::Instant::now();
        sup.shutdown();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "shutdown waited for the child to exit on its own"
        );
        assert_eq!(*reports.lock().unwrap(), 0, "a requested shutdown is not a crash");
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
            crash_reporter: None,
        };

        let sup = SidecarSupervisor::spawn(cfg).expect("spawn supervisor");
        drop(tx);
        sup.shutdown();
        let _ = rx.recv_timeout(Duration::from_secs(2));
        // If we reach here without hanging, shutdown worked.
    }
}
