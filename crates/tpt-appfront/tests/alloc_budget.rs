//! Allocation budgets: regression guards for the framework's hot paths.
//!
//! Timing is too noisy for CI, but heap allocation counts are deterministic for a given build, so
//! they make a good regression signal. Each budget is set a little above the measured count (see
//! docs/benchmarks.md). If a change pushes a path over its budget, either the change added
//! allocations (fix it) or the path got intentionally more expensive (raise the budget and say why
//! in the commit).
//!
//! All checks run inside one test. Tests in a binary run on parallel threads, and the counter is
//! global, so separate tests would count each other's allocations.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tpt_appfront::core::{create_effect, ContainerBuilder, Signal, UITree};
use tpt_appfront_html::render_page;
use tpt_appfront_test::lint_tree;

struct CountingAlloc;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Heap allocations made by one call to `f`.
fn allocations_in<F: FnOnce()>(f: F) -> usize {
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    f();
    ALLOCATIONS.load(Ordering::Relaxed) - before
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(u32),
}

fn rows(n: u32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Users");
        c.list(|list| {
            for id in 0..n {
                list.button(format!("User {id}"))
                    .key(id.to_string())
                    .on_click(Msg::Select(id));
            }
        });
    })
}

struct Budget {
    name: &'static str,
    measured: usize,
    budget: usize,
}

#[test]
fn allocation_budgets_hold() {
    let mut results = Vec::new();

    // Building a 1,000-row keyed list. Measured 2,011: one String per label and per key.
    let build = allocations_in(|| {
        std::hint::black_box(rows(1_000));
    });
    results.push(Budget {
        name: "build 1,000-row list",
        measured: build,
        budget: 2_100,
    });

    // One signal set re-runs 1 and 100 dependent effects. Measured 2 and 12. Before the fix, 100
    // effects cost 212: each run re-created its dependency wrappers and regrew its dependency list.
    for (subscribers, budget) in [(1usize, 4usize), (100, 16)] {
        let source = Signal::new(0u64);
        let runs = Rc::new(Cell::new(0u64));
        let handles: Vec<_> = (0..subscribers)
            .map(|_| {
                let source = source.clone();
                let runs = runs.clone();
                create_effect(move || {
                    std::hint::black_box(source.get());
                    runs.set(runs.get() + 1);
                })
            })
            .collect();
        // Warm up once so the first run's buffer growth is not counted.
        source.set(1);
        let count = allocations_in(|| source.set(2));
        // Each effect runs once at creation, once for the warm-up set and once for the measured set.
        assert_eq!(
            runs.get() as usize,
            subscribers * 3,
            "every effect must run on every set, or the count above measures nothing"
        );
        std::hint::black_box(&handles);
        results.push(Budget {
            name: if subscribers == 1 {
                "signal set, 1 effect"
            } else {
                "signal set, 100 effects"
            },
            measured: count,
            budget,
        });
    }

    // Server-rendered HTML for the 1,000-row list. Measured 1,021: about one String per row.
    let page = rows(1_000);
    let render = allocations_in(|| {
        std::hint::black_box(render_page(&page, "Users", "All users"));
    });
    results.push(Budget {
        name: "render_page, 1,000 rows",
        measured: render,
        budget: 1_100,
    });

    // Accessibility lint over the same list. Measured 19: keys are borrowed, not cloned.
    let lint = allocations_in(|| {
        std::hint::black_box(lint_tree(&page));
    });
    results.push(Budget {
        name: "lint_tree, 1,000 keyed rows",
        measured: lint,
        budget: 40,
    });

    let over: Vec<String> = results
        .iter()
        .filter(|b| b.measured > b.budget)
        .map(|b| {
            format!(
                "{}: {} allocations, budget {}",
                b.name, b.measured, b.budget
            )
        })
        .collect();
    for b in &results {
        eprintln!(
            "{:<32} {:>6} / {:<6} allocations",
            b.name, b.measured, b.budget
        );
    }
    assert!(
        over.is_empty(),
        "over allocation budget:\n{}",
        over.join("\n")
    );
}
