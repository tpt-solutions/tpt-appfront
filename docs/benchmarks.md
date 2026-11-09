# Benchmarks

Micro-benchmarks for the core operations. They report time per iteration and heap allocations per
iteration. Reproduce them on your own machine with:

```sh
cargo bench -p tpt-appfront --bench perf
```

The source is [`crates/tpt-appfront/benches/perf.rs`](../crates/tpt-appfront/benches/perf.rs). It
uses no external framework: each case runs a warm-up, then 15 timed samples, and reports the median.
Allocations are counted by a wrapper around the system allocator.

Allocation budgets run as a test: [`crates/tpt-appfront/tests/alloc_budget.rs`](../crates/tpt-appfront/tests/alloc_budget.rs).
Timing is too noisy for CI, but allocation counts are exact for a given build, so the budgets catch
regressions that wall-clock tests would miss or flake on.

## Reference results

Machine: Intel Core i7-12700 (20 logical cores), 16 GB RAM, Windows 11 Pro. Release profile.

| Operation | Time / iteration | Allocations / iteration |
|-----------|-----------------:|------------------------:|
| Build a 1,000-row keyed list (builder) | 350 µs | 2,011 |
| `Signal::set`, 1 dependent effect | 0.33 µs | 2 |
| `Signal::set`, 100 dependent effects | 15.8 µs | 12 |
| `render_page`, 1,000-row list | 50 µs | 1,021 |
| `lint_tree`, 1,000 keyed rows | 38 µs | 19 |
| 5-node static subtree, builder | 0.30 µs | 7 |
| 5-node static subtree, `view!` (default) | 0.50 µs | 8 |
| 5-text subtree, rebuilt | 0.60 µs | 7 |
| 5-text subtree, `static_node` cache | 0.40 µs | 6 |
| 50-text subtree, rebuilt | 3.8 µs | 55 |
| 50-text subtree, `static_node` cache | 4.0 µs | 51 |
| 500-text subtree, rebuilt | 51 µs | 508 |
| 500-text subtree, `static_node` cache | 37 µs | 501 |

Timings vary by up to about 2× between sessions on this machine, so read them as order of magnitude.
The allocation column does not vary.

## Browser results (DOM backend)

Run with `wasm-bindgen-test-runner` in headless Chrome 156, against a release wasm build.
Source: [`crates/tpt-appfront-dom/tests/bench.rs`](../crates/tpt-appfront-dom/tests/bench.rs).
The baseline is hand-written `web-sys` code that produces the same visible result with the minimum
DOM work. It is a floor, not a competitor.

| Operation | Time |
|-----------|-----:|
| Mount 1,000 keyed rows (framework) | 5,000 µs |
| Mount 1,000 rows (hand-written baseline) | 900 µs |
| Re-render, one of 1,000 labels changed (framework) | 680 µs |
| Re-render, all 1,000 labels changed (framework) | 2,950 µs |
| Rebuild all labels (hand-written baseline) | 870 µs |
| Set one label (hand-written baseline, average of 1,000 writes) | 0.4 µs |

These are from the current code, which keeps event handlers current on re-render (see below). The
earlier numbers, about 3,400 µs for a re-render, were from code that did not update handlers. They
were faster only because they were wrong.

What these show:

- **Mounting costs about 5× the hand-written baseline.** That overhead is the price of keys,
  handlers and the backend's bookkeeping. Each clickable element gets a shared handler record, so
  mounting is slower than before the handler fix.
- **A re-render with one changed label costs about 0.7 ms.** Unchanged subtrees are skipped. Each
  one is fingerprinted, and when the fingerprint matches the last render, the DOM is left alone.
  Only the handler messages are refreshed. Before this, every render walked the whole tree and
  cost about 2.3 ms.
- **A re-render with every label changed costs about 3 ms.** Nothing can be skipped there, and the
  fingerprints add a small cost (about 0.3 ms over the earlier figure).
- **Skipping trades a guarantee for speed.** A skipped subtree is not compared with the live DOM. If
  something outside the framework changes the DOM, the next render corrects it only when the
  content of that subtree changes. Inputs, selects, checkboxes and radios are never skipped, since
  their state lives in the DOM.
  Skipping unchanged subtrees (by pointer or version comparison) would make an unchanged subtree
  free. That is tracked in `todo.md`.
- **Single-node updates are cheap when they are direct.** A label write on its own takes under a
  microsecond. The cost comes from the framework walking the tree, not from DOM writes.

Timer resolution in this browser is about 5 µs, so single operations are averaged over batches.

Bug found by this benchmark: a `render` of any keyed list failed with "list item count changed"
after mount. Mounting wrapped each item in an `<li>` that the keyed diff did not expect, and
`record_mount_tree` did not record list children. Both are fixed, with regression tests in
`tests/list_update.rs`.

Two more bugs came out of the re-render work, and both are fixed:

- **Stale event handlers.** A re-render did not update event handlers. A button that kept its key
  but got a new message still sent the old one. Handlers now read their message from a slot that
  each re-render updates. A change in which handlers a node has is reported as a full replace.
  Tests: `tests/list_update.rs` and `tests/input_rebind.rs`.
- **Handler leak and wrong release.** Closures were tracked under the address of a Rust value,
  which changes when the value moves, so unmount never released them. Node identity is now a
  number stored on the DOM object.

## Signal-driven text (`reactive_text`)

Source: [`crates/tpt-appfront-dom/tests/reactive_text_bench.rs`](../crates/tpt-appfront-dom/tests/reactive_text_bench.rs).
One signal drives 1,000 text nodes. The flush is private, so two numbers are reported: the
synchronous cost of `Signal::set`, and the time from `set` until the frame that applies the update.

| Operation | Time |
|-----------|-----:|
| `Signal::set`, 1,000 bound text nodes (synchronous) | 365 µs |
| Frame with no update (baseline) | 16,660 µs (vsync) |
| Set to applied frame | 16,695 µs |
| Above the idle-frame baseline (flush and timing noise) | about 35 µs |

The flush costs about the same as one frame's timing noise, so updates are applied in the frame
after `set`. Latency is set by the frame, not by the work.

This benchmark found a scaling bug. Every effect that re-ran checked its signal's subscriber list
linearly and then unsubscribed by scanning the same list, so a set with N subscribers cost O(N²).
The first measurement was 2,355 µs. Subscribers are now tracked in a map, and a set costs O(N). At
5,000 nodes the set takes 1,155 µs, about 3× the 1,000-node figure for 5× the nodes. The fix is in
`tpt-appfront-core` and covers every signal, not only text. Regression tests:
`crates/tpt-appfront-core/tests/subscriber_reuse.rs`.

## What changed, and why it was measured

**Signal fan-out: 212 → 12 allocations per `set` with 100 effects.** Two causes. Each dependency was
registered as an `Rc` wrapped in a second `Rc`, and each effect run dropped its dependency list's
capacity and regrew it. Now the existing `Rc` is unsized directly, and the list keeps its buffer.
Time went from about 21 µs to about 16 µs.

**Lint: 2,019 → 19 allocations for 1,000 keyed rows, and about 3× faster.** `lint_tree` cloned every
key twice: once per node and once into the duplicate-key set. Keys are now borrowed, and cloned only
when an issue is reported.

**Static hoisting: investigated, and the answer is "it depends", so the default is unchanged.**
The `view!` macro can hoist provably static subtrees into a build-once cache, which returns a clone
of the cached tree on each call. The investigation asked whether that wins.

- The clone allocates as many times as the builder does. For 500 text nodes, the cache makes 501
  allocations and rebuilding makes 508. The cache saves no allocations; it only skips the building
  code.
- Time is the only place it might win, and that varied between runs. Across sessions, the cache
  looked slower at 500 nodes once and faster twice. The builder's own baseline moved by up to 2× in
  the same period, so the timing comparison does not support a conclusion either way.
- At 5 and 50 nodes the two are within noise.

So the honest conclusion is: no measured benefit that holds up, and no measured harm that holds up
either. A reliable answer needs a quieter machine, or a version of the cache that shares the tree
rather than cloning it. Shared trees need a change to the AST, which is open work in `todo.md`.

## What these numbers do not show

- **No competitor comparison.** These are AppFront-only figures. Comparing with React, Leptos or
  Dioxus would need the same workload measured on the same machine, which has not been done.
- **No browser timings.** DOM mount and patch costs depend on the browser. These figures cover the
  Rust side only.
- **Synthetic workloads.** The list is uniform text rows. Real screens have more varied nodes.
- **One machine.** Run the benchmark on your own hardware before making decisions from it.

## Adding a benchmark

Use `bench(name, iterations, closure)` in `perf.rs`. Keep the workload deterministic, and wrap
results in `black_box` so the compiler cannot remove the work. If a path matters for performance,
add its allocation count to `tests/alloc_budget.rs` as well.
