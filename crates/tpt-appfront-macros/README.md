# tpt-appfront-macros

The procedural macros for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

This crate is a `proc-macro` crate. Its three macros are re-exported by
`tpt-appfront-core`, so most users never depend on it directly:

- **`#[component]`** (`tpt_appfront_core::component`): wraps a function that returns
  `UITree<Msg>`. If the function does not set them, it fills in `meta.class` (the
  kebab-cased function name) and `meta.ai.description` (from the doc comment) on the
  root node. `#[component(memo)]` caches the built tree per props value.
  It also sets `meta.is_dynamic` with a token-level heuristic that looks for signal
  reads. This is a hint, not a dataflow analysis.
- **`view!`** (`tpt_appfront_core::view`) and **`rsx!`** (`tpt_appfront_core::rsx`,
  an alias): an HTML-like template macro that expands to `UITree::container` builder
  calls. Supported tags are `Container`, `Heading`, `Text`, `Button`, `Input`,
  `Textarea`, `Checkbox`, `Select`, `Radio`, `List`, `DataGrid`, `Image`, `Link`,
  and `Media`. It supports `{ expr }` attributes and children, `{if}` and `{for}`
  control flow, and two-way binding through `on_input` (on `Input`, `Textarea`,
  `Select`, `Radio`) and `on_toggle` (on `Checkbox`). Statically known subtrees are
  hoisted into a cache built once through `static_tree::static_node`.
- Custom components: a self-closing, capitalized tag such as `<LoginForm .../>`
  expands to a call into the templates crate. The expansion resolves that crate the same
  way as core: `tpt-appfront-templates` if the calling crate depends on it, otherwise the
  `tpt-appfront` umbrella's `templates` feature. Other non-built-in composition goes
  through `{ expr }`.

## Install

Depend on `tpt-appfront-core`, which re-exports the macros:

```toml
[dependencies]
tpt-appfront-core = "0.1"
```

## Example

```rust
use tpt_appfront_core::{view, Signal, UITree};

#[derive(Debug, Clone)]
enum Msg { Increment }

fn counter(count: &Signal<i32>) -> UITree<Msg> {
    view! {
        <Container>
            <Heading level={1u8}>"Counter"</Heading>
            <Text>{ format!("Count: {}", count.get()) }</Text>
            <Button on_click={Msg::Increment}>"+1"</Button>
        </Container>
    }
}
```

The macros only add syntax. The output is the same `UITree` you would build with
the builder API, so there is no runtime overhead beyond that API.

## License

MIT OR Apache-2.0
