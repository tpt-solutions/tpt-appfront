# tpt-appfront-macros

`#[component]` attribute macro and the `view!`/`rsx!` templating macro for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

`#[component]` (re-exported as `tpt_appfront_core::component`) wraps a `UITree`-returning fn and auto-fills `meta.class`/`meta.ai.description` on its root node. `view!`/`rsx!` (re-exported as `tpt_appfront_core::view`) is an HTML-like template macro that detects provably-static subtrees and hoists them into a build-once cache instead of rebuilding every render.

This crate is re-exported through `tpt-appfront-core` — you shouldn't normally depend on it directly:

```toml
[dependencies]
tpt-appfront-core = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme).

## License

Apache-2.0
