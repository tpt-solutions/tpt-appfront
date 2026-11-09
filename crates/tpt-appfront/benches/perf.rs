//! Micro-benchmarks for the core paths. Run with `cargo bench -p tpt-appfront --bench perf`.
//!
//! No external benchmark framework. Each case runs a warm-up, then 15 timed samples, and reports
//! the median time per iteration. It also reports heap allocations per iteration, counted by a
//! wrapper around the system allocator. Allocation counts are deterministic for a given build, so
//! they stay comparable when wall-clock times drift between runs on a noisy machine.
//!
//! Timings still depend on the machine, build profile and background load. Compare runs from the
//! same machine, and read docs/benchmarks.md before drawing conclusions.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tpt_appfront::core::static_tree::static_node;
use tpt_appfront::core::{create_effect, ContainerBuilder, Signal, UITree};
use tpt_appfront::prelude::view;
use tpt_appfront_html::render_page;
use tpt_appfront_test::lint_tree;

// ---------------------------------------------------------------------------
// Allocation counting
// ---------------------------------------------------------------------------

struct CountingAlloc;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged; the counter is an atomic.
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

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(u32),
}

const SAMPLES: usize = 15;

/// Times `f` over `iters` iterations, per sample, and counts allocations for one iteration.
/// Returns (median time per iteration, allocations per iteration).
fn measure<F: FnMut()>(iters: u32, mut f: F) -> (Duration, usize) {
    for _ in 0..iters.min(50) {
        f();
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    f();
    let allocs = ALLOCATIONS.load(Ordering::Relaxed) - before;

    let mut samples: Vec<Duration> = (0..SAMPLES)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..iters {
                f();
            }
            start.elapsed() / iters
        })
        .collect();
    samples.sort();
    (samples[SAMPLES / 2], allocs)
}

fn bench<F: FnMut()>(name: &str, iters: u32, f: F) {
    let (time, allocs) = measure(iters, f);
    println!(
        "{name:<46} {:>11.3} µs {allocs:>9} allocs",
        time.as_secs_f64() * 1e6
    );
}

// ---------------------------------------------------------------------------
// Workloads
// ---------------------------------------------------------------------------

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

/// A static subtree written with `view!`. The default codegen builds it on each call.
fn static_panel() -> UITree<Msg> {
    view! {
        <Container class="p-4">
            <Heading level={1u8}>"Settings"</Heading>
            <Text>"Account, billing and notification preferences."</Text>
            <Text>"Changes are saved automatically."</Text>
            <Text>"Contact support for anything else."</Text>
            <Button on_click={ Msg::Select(0) }>"Save"</Button>
        </Container>
    }
}

/// The same static subtree built with the builder API on every call.
fn static_panel_builder() -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Settings");
        c.text("Account, billing and notification preferences.");
        c.text("Changes are saved automatically.");
        c.text("Contact support for anything else.");
        c.button("Save").on_click(Msg::Select(0));
    })
}

fn text_rows(n: u32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        for i in 0..n {
            c.text(format!("Line {i}"));
        }
    })
}

/// Caches `text_rows(n)` the way the `view!` codegen does. Each expansion site gets its own
/// `static`, which gives it its own cache key, so every size needs its own site.
macro_rules! cached_text_rows {
    ($n:expr) => {{
        static SENTINEL: u8 = 0;
        let id = (&SENTINEL as *const u8) as u64;
        static_node(id, || text_rows($n))
    }};
}

fn main() {
    println!(
        "TPT AppFront micro-benchmarks (median of {SAMPLES} samples; allocations per iteration)\n"
    );

    // Building a 1,000-row list.
    bench("build 1,000-row list (builder)", 20, || {
        black_box(rows(1_000));
    });

    // Signal fan-out: one set() that re-runs N dependent effects.
    for &subscribers in &[1usize, 100] {
        let source = Signal::new(0u64);
        let runs = Rc::new(Cell::new(0u64));
        let handles: Vec<_> = (0..subscribers)
            .map(|_| {
                let source = source.clone();
                let runs = runs.clone();
                create_effect(move || {
                    black_box(source.get());
                    runs.set(runs.get() + 1);
                })
            })
            .collect();
        let mut value = 0u64;
        bench(
            &format!("signal set, {subscribers:>3} dependent effect(s)"),
            2_000,
            || {
                value += 1;
                source.set(value);
            },
        );
        // Every effect must have run for each set; otherwise the number above measures nothing.
        assert!(runs.get() > 0, "effects did not run");
        black_box(&handles);
    }

    // Static subtrees: the view! default, the builder, and the build-once cache by size. The
    // cache clones the stored tree on every call, so its cost grows with the tree.
    bench("view! static 5-node subtree (default)", 2_000, || {
        black_box(static_panel());
    });
    bench("builder static 5-node subtree", 2_000, || {
        black_box(static_panel_builder());
    });
    // One expansion site per size; see `cached_text_rows!`.
    bench("rebuild    5-text subtree", 2_000, || {
        black_box(text_rows(5));
    });
    bench("cached     5-text subtree (static_node)", 2_000, || {
        black_box(cached_text_rows!(5));
    });
    bench("rebuild   50-text subtree", 2_000, || {
        black_box(text_rows(50));
    });
    bench("cached    50-text subtree (static_node)", 2_000, || {
        black_box(cached_text_rows!(50));
    });
    bench("rebuild  500-text subtree", 200, || {
        black_box(text_rows(500));
    });
    bench("cached   500-text subtree (static_node)", 200, || {
        black_box(cached_text_rows!(500));
    });

    // Server-rendered HTML and linting over the 1,000-row list.
    let page = rows(1_000);
    bench("render_page, 1,000-row list", 10, || {
        black_box(render_page(&page, "Users", "All users"));
    });
    bench("lint_tree, 1,000-row list", 10, || {
        black_box(lint_tree(&page));
    });
}
