//! `Signal::persisted` keeps values across restarts while dev-state persistence
//! is on, and behaves like `Signal::new` otherwise.
//!
//! One test function, because the switch is a process-wide environment variable
//! and parallel tests would race on it.

use tpt_appfront_core::dev_state::DEV_STATE_ENV;
use tpt_appfront_core::Signal;

#[test]
fn persisted_signals_survive_restart() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("dev-state-test.json");
    let _ = std::fs::remove_file(&path);

    // Off: the initial value, and writes are not saved anywhere.
    std::env::remove_var(DEV_STATE_ENV);
    let off: Signal<i32> = Signal::persisted("count", 7);
    assert_eq!(off.get(), 7);
    off.set(9);
    assert_eq!(off.get(), 9);
    assert!(
        !path.exists(),
        "no file is written while persistence is off"
    );

    // On: a write is saved under its name.
    std::env::set_var(DEV_STATE_ENV, &path);
    let first: Signal<i32> = Signal::persisted("count", 0);
    assert_eq!(first.get(), 0, "nothing saved yet, so the initial value");
    first.set(41);
    drop(first);
    assert!(path.exists(), "the write is saved to the dev-state file");

    // A restart (a new signal with the same name) reads it back.
    let second: Signal<i32> = Signal::persisted("count", 0);
    assert_eq!(second.get(), 41, "the saved value survives the restart");

    // A saved value that no longer fits the type falls back to the initial value.
    let retyped: Signal<String> = Signal::persisted("count", "fresh".to_string());
    assert_eq!(retyped.get(), "fresh");

    // Saving one key keeps the others.
    let name: Signal<String> = Signal::persisted("name", "a".to_string());
    name.set("b".to_string());
    let again: Signal<i32> = Signal::persisted("count", 0);
    assert_eq!(again.get(), 41, "saving `name` leaves `count` alone");
    let name_again: Signal<String> = Signal::persisted("name", "a".to_string());
    assert_eq!(name_again.get(), "b");

    std::env::remove_var(DEV_STATE_ENV);
    let _ = std::fs::remove_file(&path);
}
