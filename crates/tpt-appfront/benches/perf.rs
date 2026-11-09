//! Micro-benchmarks for the core paths. Run with `cargo bench -p tpt-appfront`.
//!
//! No external benchmark framework: each case runs a warm-up, then 15 timed samples, and the
//! median per-iteration time is reported. Numbers depend on the machine, the build profile and
//! background load, so compare runs from the same machine rather than across machines.

use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;
use std::time::{Duration, Instant};

use tpt_appfront::core::{create_effect, ContainerBuilder, Signal, UITree};
use tpt_appfront::prelude::view;
use tpt_appfront_html::render_page;
use tpt_appfront_test::lint_tree;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(u32),
}

const SAMPLES: usize = 15;

/// Runs `f` `iters` times per sample and returns the median per-iteration time.
fn measure<F: FnMut()>(iters: u32, mut f: F) -> Duration {
    for _ in 0..iters.min(50) {
        f();
    }
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
    samples[SAMPLES / 2]
}

fn report(name: &str, per_iter: Duration) {
    println!("{name:<48} {:>12.3} µs", per_iter.as_secs_f64() * 1e6);
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

/// A static subtree written with `view!`. The macro hoists it into a build-once cache, so
/// repeated calls return the cached tree.
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

fn main() {
    println!("TPT AppFront micro-benchmarks (median of {SAMPLES} samples, per iteration)\n");

    // 1. Building a 1,000-row list.
    let t = measure(20, || {
        black_box(rows(1_000));
    });
    report("build 1,000-row list (builder)", t);

    // 2. Signal fan-out: one set() that re-runs N dependent effects.
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
        let t = measure(2_000, || {
            value += 1;
            source.set(value);
        });
        black_box(&handles);
        report(&format!("signal set, {subscribers:>3} dependent effect(s)"), t);
    }

    // 3. Static subtree: view! hoisting vs rebuilding with the builder.
    let t = measure(2_000, || {
        black_box(static_panel());
    });
    report("static 5-node subtree via view! (hoisted)", t);
    let t = measure(2_000, || {
        black_box(static_panel_builder());
    });
    report("static 5-node subtree via builder", t);

    // 4. Server-rendered HTML for the 1,000-row list.
    let page = rows(1_000);
    let t = measure(10, || {
        black_box(render_page(&page, "Users", "All users"));
    });
    report("render_page, 1,000-row list", t);

    // 5. Accessibility lint over the same list.
    let t = measure(10, || {
        black_box(lint_tree(&page));
    });
    report("lint_tree, 1,000-row list", t);
}
