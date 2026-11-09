//! Subscriptions must survive effects being dropped and new effects reusing their memory.

use std::cell::Cell;
use std::rc::Rc;

use tpt_appfront_core::{create_effect, Signal};

#[test]
fn effects_created_after_others_are_dropped_still_react() {
    let source = Signal::new(0u32);
    for round in 0..200u32 {
        // An effect that is dropped at the end of the round. Its memory may be reused below.
        let dropped = create_effect({
            let source = source.clone();
            move || {
                std::hint::black_box(source.get());
            }
        });
        drop(dropped);

        let seen = Rc::new(Cell::new(u32::MAX));
        let handle = create_effect({
            let source = source.clone();
            let seen = seen.clone();
            move || seen.set(source.get())
        });
        source.set(round);
        assert_eq!(
            seen.get(),
            round,
            "round {round}: the live effect must update"
        );
        drop(handle);
    }
}

#[test]
fn many_subscribers_all_run_once_per_set() {
    let source = Signal::new(0u32);
    let runs = Rc::new(Cell::new(0u32));
    let handles: Vec<_> = (0..500)
        .map(|_| {
            let source = source.clone();
            let runs = runs.clone();
            create_effect(move || {
                std::hint::black_box(source.get());
                runs.set(runs.get() + 1);
            })
        })
        .collect();
    assert_eq!(runs.get(), 500, "each effect runs once on creation");

    source.set(1);
    assert_eq!(
        runs.get(),
        1000,
        "each effect runs exactly once per set, with no duplicates"
    );

    source.set(2);
    assert_eq!(runs.get(), 1500);
    drop(handles);
}
