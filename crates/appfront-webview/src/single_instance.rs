//! Cross-process single-instance enforcement for the webview shell.
//!
//! Titan must never launch two copies of itself (each would spawn its own
//! sidecar + window). We take an exclusive OS lock on a file under the app's
//! data directory; if a previous lock holder exists we report the existing
//! instance's "show" request (via a small handshake file) and refuse to start.

use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Resolves the lock directory, derived from the app identifier so two
/// different apps don't collide.
fn lock_dir(app_id: &str) -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("appfront")
        .join(app_id)
}

/// Result of a single-instance check.
pub enum InstanceCheck {
    /// This process is the sole instance; the returned guard must be kept alive
    /// for the lifetime of the app (dropping it releases the lock).
    Primary(InstanceGuard),
    /// Another instance is already running. `show_request` points at a file the
    /// primary instance watches to honour a "focus the existing window" request.
    Secondary {
        /// Path of the show-request handshake file.
        show_request: PathBuf,
    },
}

/// Owns the lock file; releasing the lock happens on drop.
pub struct InstanceGuard {
    _file: File,
    _path: PathBuf,
}

/// Enforces a single running instance identified by `app_id`.
///
/// On success returns [`InstanceCheck::Primary`] with a guard that holds the
/// lock open. On failure (another instance alive) returns
/// [`InstanceCheck::Secondary`] after writing a show-request signal so the
/// existing instance can raise its window.
pub fn ensure_single_instance(app_id: &str) -> Result<InstanceCheck> {
    let dir = lock_dir(app_id);
    fs::create_dir_all(&dir).context("create single-instance lock dir")?;
    let lock_path = dir.join("instance.lock");
    let show_request = dir.join("show.request");

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .context("open single-instance lock file")?;

    match file.try_lock_exclusive() {
        Ok(()) => Ok(InstanceCheck::Primary(InstanceGuard {
            _file: file,
            _path: lock_path,
        })),
        Err(_) => {
            // Another instance holds the lock. Signal it to show.
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&show_request)
            {
                let _ = f.write_all(b"show");
            }
            Ok(InstanceCheck::Secondary { show_request })
        }
    }
}

/// Blocks until the primary instance's show-request file appears (the
/// secondary signalled it), then returns. Used by the primary to wait for a
/// "focus me" nudge from a later launch attempt.
pub fn wait_for_show_request(show_request: &Path) -> Result<()> {
    // Best-effort: poll the handshake file. A real impl would use a platform
    // file-change watcher; polling is sufficient and dependency-free.
    loop {
        if show_request.exists() {
            let mut buf = String::new();
            if let Ok(mut f) = File::open(show_request) {
                let _ = f.read_to_string(&mut buf);
            }
            if buf.contains("show") {
                let _ = fs::remove_file(show_request);
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}
