# Benchmarks

Micro-benchmarks for the core operations: building a tree, updating a signal, rendering HTML and
linting. They run in release mode, and you can reproduce them on your own machine with one
command:

```sh
cargo bench -p tpt-appfront --bench perf
```

The source is [`crates/tpt-appfront/benches/perf.rs`](../crates/tpt-appfront/benches/perf.rs). It
uses no external framework. Each case runs a warm-up, then 15 timed samples, and reports the median
time per iteration.

## Reference results

| Machine | OS | Build |
|---------|----|-------|
| Intel Core i7-12700 (20 logical cores), 16 GB RAM | Windows 11 Pro | `cargo bench` (release profile), one run |

| Operation | Time per iteration |
|-----------|-------------------:|
| Build a 1,000-row keyed list (builder API) | 218.5 µs |
| `Signal::set` with 1 dependent effect | 0.39 µs |
| `Signal::set` with 100 dependent effects | 24.8 µs |
| 5-node static subtree via `view!` (hoisted) | 0.57 µs |
| 5-node static subtree via the builder API | 0.33 µs |
| `render_page` (semantic HTML), 1,000-row list | 69.3 µs |
| `lint_tree`, 1,000-row list | 135.6 µs |

## What the numbers say

- **Signal updates are cheap, and fan-out is linear.** A set with one dependent effect costs about
  0.4 µs. With 100 dependents it costs about 25 µs, roughly 0.25 µs per effect. Only the effects
  that read the changed signal run.
- **Building a large list is the expensive part.** A 1,000-row list takes about 0.2 ms to build.
  The DOM backend diffs the rebuilt tree against the mounted one and keeps unchanged nodes, so
  you rebuild the view, not the page. Use keys on list rows so a reorder updates only the rows
  that moved.
- **Static hoisting did not win here, and this is a real result.** For a 5-node static subtree,
  the `view!` version is about 1.7× slower than building the same tree with the builder. The
  hoisted subtree is stored in a cache and cloned on each call. The clone costs more than building
  five nodes. Hoisting should pay off for larger static subtrees, or when the subtree is expensive
  to build. This is not yet measured, so do not rely on hoisting for speed until it is. A
  follow-up in `todo.md` tracks it.
- **HTML rendering and linting scale linearly.** Both take well under a millisecond for a
  1,000-row page, so they are cheap enough to run on every server response in development.

## What these numbers do not show

- **No competitor comparison.** These are AppFront-only timings. Comparing with React, Leptos or
  Dioxus would need the same workload in each framework, measured on the same machine. That work
  is not done, so there are no such claims here.
- **No browser timings.** The DOM backend's mount and patch costs depend on the browser's layout
  and DOM engine. The numbers above cover the Rust side only.
- **One machine, one run.** Treat the numbers as an order of magnitude. Run the benchmark on your
  own hardware before you make a decision based on it.
- **Synthetic workloads.** The list is uniform text rows. Real screens have more varied nodes, so
  the costs will differ.

## Adding a benchmark

Add a function that takes a closure and passes it to `measure`, then call `report` with the name.
Keep the workload deterministic, and use `black_box` on results so the compiler cannot remove the
work.
