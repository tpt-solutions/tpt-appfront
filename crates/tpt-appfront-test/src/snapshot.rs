//! Snapshot assertions: compare rendered output with a committed `.snap` file.
//!
//! ```ignore
//! let h = Harness::new(counter_ui());
//! tpt_appfront_test::assert_snapshot!("counter_initial", h.snapshot());
//! ```
//!
//! Snapshots are stored in `tests/snapshots/<name>.snap` relative to the calling crate.
//!
//! - A missing snapshot is written from the actual output, and the assertion fails once. Review
//!   the new file and commit it.
//! - `TPT_UPDATE_SNAPSHOTS=1 cargo test` overwrites every snapshot that differs.
//! - Line endings are normalised to `\n`, so a snapshot written on one platform compares equal on
//!   another (Git's `autocrlf` can change them).

use std::fs;
use std::path::{Path, PathBuf};

/// Environment variable that makes snapshot assertions overwrite stored output.
pub const UPDATE_ENV: &str = "TPT_UPDATE_SNAPSHOTS";

/// Compares `actual` with the snapshot `name` under `dir`. Panics on a mismatch or a new snapshot.
///
/// Most callers use [`assert_snapshot!`](crate::assert_snapshot), which fills in `dir`.
pub fn assert_snapshot_in(dir: &Path, name: &str, actual: &str) {
    let path = snapshot_path(dir, name);
    let actual = normalise(actual);
    let update = std::env::var_os(UPDATE_ENV).is_some_and(|v| !v.is_empty());

    let stored = fs::read_to_string(&path).ok().map(|s| normalise(&s));
    match stored {
        _ if update => write_snapshot(&path, &actual),
        Some(stored) if stored == actual => {}
        Some(stored) => panic!(
            "snapshot `{name}` does not match {}\n\n--- stored\n{stored}\n+++ actual\n{actual}\n\
             Rerun with {UPDATE_ENV}=1 to accept the new output.",
            path.display()
        ),
        None => {
            write_snapshot(&path, &actual);
            panic!(
                "new snapshot `{name}` written to {}. Review it and commit it, then rerun.",
                path.display()
            );
        }
    }
}

/// The path of snapshot `name` under `dir`: `<dir>/<name>.snap`.
pub fn snapshot_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.snap"))
}

fn write_snapshot(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create {}: {e}", parent.display()));
    }
    fs::write(path, content).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn normalise(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Asserts that `actual` matches the stored snapshot `name`. See the [`snapshot`](crate::snapshot)
/// module for the storage layout and the update flag.
#[macro_export]
macro_rules! assert_snapshot {
    ($name:expr, $actual:expr $(,)?) => {
        $crate::snapshot::assert_snapshot_in(
            ::std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots")),
            $name,
            &$actual,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system temp dir, unique per test.
    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tpt-snapshot-{}-{}", test, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_new_snapshot_is_written_and_the_assertion_fails_once() {
        let dir = scratch("new");
        let result = std::panic::catch_unwind(|| assert_snapshot_in(&dir, "first", "hello\n"));
        assert!(
            result.is_err(),
            "first run must fail so the file is reviewed"
        );
        assert_eq!(
            fs::read_to_string(snapshot_path(&dir, "first")).unwrap(),
            "hello\n"
        );
        // The second run matches and passes.
        assert_snapshot_in(&dir, "first", "hello\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_output_fails_with_the_stored_and_actual_text() {
        let dir = scratch("changed");
        fs::create_dir_all(&dir).unwrap();
        fs::write(snapshot_path(&dir, "c"), "old\n").unwrap();
        let err = std::panic::catch_unwind(|| assert_snapshot_in(&dir, "c", "new\n"))
            .expect_err("a changed output must fail");
        let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(msg.contains("old") && msg.contains("new"), "{msg}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crlf_in_the_stored_file_still_matches() {
        let dir = scratch("crlf");
        fs::create_dir_all(&dir).unwrap();
        fs::write(snapshot_path(&dir, "eol"), "a\r\nb\r\n").unwrap();
        assert_snapshot_in(&dir, "eol", "a\nb\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
