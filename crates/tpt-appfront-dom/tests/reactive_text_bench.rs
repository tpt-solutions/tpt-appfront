//! Benchmark for signal-driven text (`reactive_text`), run in headless Chrome under wasm-bindgen-test.
//!
//! Two numbers:
//! - `set` cost: the synchronous time for `Signal::set` to re-run the effect of every bound text
//!   node and queue its update. This is the work done on the caller's thread.
//! - Set-to-frame latency: the time from `set` until a `requestAnimationFrame` callback registered
//!   right after it runs. The flush is queued by the first update, so it runs before that callback.
//!   Compared with a frame that has no update, the difference is the flush work plus frame timing
//!   noise. Read it as an upper bound.
//!
//! The flush itself is private, so it is not timed on its own.

#![cfg(target_arch = "wasm32")]

use tpt_appfront_core::Signal;
use tpt_appfront_dom::reactive_text;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Document, Element};

wasm_bindgen_test_configure!(run_in_browser);

const TEXTS: usize = 1_000;
const SAMPLES: usize = 15;

fn document() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

fn now() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}

/// Resolves on the next animation frame.
async fn next_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        web_sys::window()
            .unwrap()
            .request_animation_frame(resolve.unchecked_ref())
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

fn report(name: &str, ms: f64) {
    wasm_bindgen_test::console_log!("{name:<52} {:>12.1} µs", ms * 1_000.0);
}

#[wasm_bindgen_test]
async fn reactive_text_benchmarks() {
    let doc = document();
    let parent: Element = doc.create_element("div").unwrap();
    let source = Signal::new("a".to_string());
    let mut nodes = Vec::new();
    let mut handles = Vec::new();
    for _ in 0..TEXTS {
        let (node, handle) = reactive_text(&doc, source.clone()).unwrap();
        parent.append_child(&node).unwrap();
        nodes.push(node);
        handles.push(handle);
    }
    // Let the initial frame settle before measuring.
    next_frame().await;

    // Baseline: a frame with no update.
    let mut idle = Vec::new();
    for _ in 0..SAMPLES {
        let start = now();
        next_frame().await;
        idle.push(now() - start);
    }
    let idle_ms = median(idle);

    // `set` cost and set-to-frame latency. Values alternate so every set changes every text.
    let mut set_cost = Vec::new();
    let mut set_to_frame = Vec::new();
    for i in 0..SAMPLES {
        let value = if i % 2 == 0 { "b" } else { "a" };
        let start = now();
        source.set(value.to_string());
        set_cost.push(now() - start);
        next_frame().await;
        set_to_frame.push(now() - start);

        // Correctness: every bound node shows the new value once the frame has run.
        let first = nodes[0].text_content().unwrap_or_default();
        let last = nodes[TEXTS - 1].text_content().unwrap_or_default();
        assert_eq!(first, value, "first node updated");
        assert_eq!(last, value, "last node updated");
    }

    report("frame with no update (baseline)", idle_ms);
    report(
        &format!("Signal::set, {TEXTS} bound text nodes (synchronous)"),
        median(set_cost),
    );
    let latency_ms = median(set_to_frame);
    report(
        &format!("set to applied frame, {TEXTS} text nodes"),
        latency_ms,
    );
    report(
        "  above the idle-frame baseline (flush + noise)",
        (latency_ms - idle_ms).max(0.0),
    );
    drop(handles);
}
