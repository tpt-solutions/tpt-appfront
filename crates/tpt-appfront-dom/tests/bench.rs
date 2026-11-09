//! Browser benchmarks for the DOM backend. Run under `wasm-bindgen-test-runner` (headless Chrome):
//!
//! ```sh
//! cargo test -p tpt-appfront-dom --target wasm32-unknown-unknown --test bench --no-run
//! WASM_BINDGEN_TEST_RUNNER_CHROME=<path to chrome> wasm-bindgen-test-runner \
//!     target/wasm32-unknown-unknown/debug/deps/bench-*.wasm
//! ```
//!
//! What is measured. Each case is the median of 15 samples, timed with `performance.now()`.
//! The timed region covers the work the framework does: `mount` or `render` on the DOM backend.
//! Building the `UITree` is done outside the timed region, since that cost is paid in every
//! backend and is covered by the native benches.
//!
//! The baseline is hand-written `web-sys` code that produces the same visible result with the
//! minimum DOM work. It is the floor the framework can approach, not a competitor.
//!
//! Timings depend on the browser, the machine and background load, so read them as order of
//! magnitude and compare runs from the same machine. A failing assertion here is a correctness
//! bug, not a slow run.

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_dom::{mount, MountedRoot};
use wasm_bindgen_test::*;
use web_sys::{Document, Element};

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(u32),
}

const ROWS: u32 = 1_000;
const SAMPLES: usize = 15;

fn document() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

fn now() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}

fn dispatch() -> Rc<dyn Fn(Msg)> {
    Rc::new(|_| {})
}

/// A 1,000-row keyed list. Every label is `"User {id} {label}"`, and the row `changed` (if any)
/// gets a trailing `!`, so two trees that differ only in `changed` differ in exactly one label.
fn rows(label: &str, changed: Option<u32>) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Users");
        c.list(|list| {
            for id in 0..ROWS {
                let text = if changed == Some(id) {
                    format!("User {id} {label}!")
                } else {
                    format!("User {id} {label}")
                };
                list.button(text)
                    .key(id.to_string())
                    .on_click(Msg::Select(id));
            }
        });
    })
}

/// The hand-written baseline: the same visible structure (a heading and 1,000 buttons) built
/// with direct `web-sys` calls, with no keys, handlers or diffing.
/// Returns the container and the button at index `watch` (the one the update case changes).
fn baseline_mount(doc: &Document, labels: &[String], watch: usize) -> (Element, Element) {
    let container = doc.create_element("div").unwrap();
    let heading = doc.create_element("h1").unwrap();
    heading.set_text_content(Some("Users"));
    container.append_child(&heading).unwrap();
    let list = doc.create_element("div").unwrap();
    let mut watched = None;
    for (i, label) in labels.iter().enumerate() {
        let button = doc.create_element("button").unwrap();
        button.set_text_content(Some(label));
        list.append_child(&button).unwrap();
        if i == watch {
            watched = Some(button);
        }
    }
    container.append_child(&list).unwrap();
    (container, watched.expect("watch index in range"))
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

/// Prints a result in microseconds. Single label writes take well under a millisecond, so
/// milliseconds with three decimals would round them to zero.
fn report(name: &str, ms: f64) {
    wasm_bindgen_test::console_log!("{name:<46} {:>12.1} µs", ms * 1_000.0);
}

fn label_texts(label: &str) -> Vec<String> {
    (0..ROWS).map(|id| format!("User {id} {label}")).collect()
}

#[wasm_bindgen_test]
fn dom_benchmarks() {
    let doc = document();
    wasm_bindgen_test::console_log!("\nDOM backend benchmarks (median of {SAMPLES} samples)\n");

    // 1. Mounting the 1,000-row list into a detached container.
    let mut mount_times = Vec::new();
    for _ in 0..SAMPLES {
        let container = doc.create_element("div").unwrap();
        let ui = rows("a", None);
        let start = now();
        let root: MountedRoot<Msg> = mount(&container, &ui, dispatch()).unwrap();
        mount_times.push(now() - start);
        root.unmount();
    }
    let mount_ms = median(mount_times);
    report("mount 1,000 keyed rows (framework)", mount_ms);

    let mut base_mount_times = Vec::new();
    for _ in 0..SAMPLES {
        let labels = label_texts("a");
        let start = now();
        let (node, _) = baseline_mount(&doc, &labels, 7);
        base_mount_times.push(now() - start);
        std::hint::black_box(node);
    }
    let base_mount_ms = median(base_mount_times);
    report("mount 1,000 rows (hand-written baseline)", base_mount_ms);

    // 2. Re-rendering a tree in which one row's label changed.
    let container = doc.create_element("div").unwrap();
    let mut root = mount(&container, &rows("a", None), dispatch()).unwrap();
    let mut one_changed = Vec::new();
    for i in 0..SAMPLES {
        let next = if i % 2 == 0 {
            rows("a", Some(7))
        } else {
            rows("a", None)
        };
        let start = now();
        root.render(&next).unwrap();
        one_changed.push(now() - start);
    }
    root.unmount();
    let one_ms = median(one_changed);
    report("render, one of 1,000 labels changed (framework)", one_ms);

    let (_base_container, watched) = baseline_mount(&doc, &label_texts("a"), 7);
    // One label write takes far less than the timer's resolution, so each sample performs
    // `BATCH` writes and reports the average.
    const BATCH: u32 = 1_000;
    let mut base_one = Vec::new();
    for _ in 0..SAMPLES {
        let start = now();
        for k in 0..BATCH {
            let text = if k % 2 == 0 { "User 7 a!" } else { "User 7 a" };
            watched.set_text_content(Some(text));
        }
        base_one.push((now() - start) / BATCH as f64);
    }
    let base_one_ms = median(base_one);
    report("set one label (hand-written baseline)", base_one_ms);

    // 3. Re-rendering a tree in which every label changed.
    let container = doc.create_element("div").unwrap();
    let mut root = mount(&container, &rows("a", None), dispatch()).unwrap();
    let mut all_changed = Vec::new();
    for i in 0..SAMPLES {
        let next = if i % 2 == 0 {
            rows("b", None)
        } else {
            rows("a", None)
        };
        let start = now();
        root.render(&next).unwrap();
        all_changed.push(now() - start);
    }
    root.unmount();
    let all_ms = median(all_changed);
    report("render, all 1,000 labels changed (framework)", all_ms);

    let mut base_all = Vec::new();
    for i in 0..SAMPLES {
        let labels = if i % 2 == 0 {
            label_texts("b")
        } else {
            label_texts("a")
        };
        let start = now();
        let (node, _) = baseline_mount(&doc, &labels, 7);
        base_all.push(now() - start);
        std::hint::black_box(node);
    }
    let base_all_ms = median(base_all);
    report("rebuild all labels (hand-written baseline)", base_all_ms);

    // Sanity: a benchmark that did no work would be meaningless. The mounted tree must contain
    // every row.
    let check = doc.create_element("div").unwrap();
    let root = mount(&check, &rows("a", None), dispatch()).unwrap();
    let html = check.inner_html();
    let buttons = html.matches("<button").count();
    assert_eq!(
        buttons, ROWS as usize,
        "framework mount must create one button per row"
    );
    root.unmount();
}
