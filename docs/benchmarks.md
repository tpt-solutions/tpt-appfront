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
