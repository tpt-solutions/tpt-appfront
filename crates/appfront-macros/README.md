# appfront-macros

`#[component]` and `view!`/`rsx!` proc macros for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

`#[appfront::component]` wraps a `UITree`-returning function and auto-fills `meta.class` (kebab-cased from the function name) and `meta.ai.description` (from the doc comment) on the root node, using a token-level heuristic to flag static vs. dynamic content. `view!`/`rsx!` is an HTML-like template macro covering `Container`/`Heading`/`Text`/`Button`/`Input`, which precisely detects provably-static subtrees and hoists them into `appfront_core::static_tree`'s build-once cache instead of rebuilding them on every render.

Both macros are re-exported from `appfront-core` (as `appfront_core::component` and `appfront_core::view`) — most users won't depend on this crate directly.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture.
