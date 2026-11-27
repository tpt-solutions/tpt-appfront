# The view! template

`view!` writes a `UITree` in HTML-like markup, and it is the usual way to write views.

```rust,ignore
use tpt_appfront_core::view;

let ui = view! {
    <Container>
        <Heading level={1u8}>"Counter"</Heading>
        <Text>{ format!("Count: {}", count) }</Text>
        <Button on_click={Msg::Increment}>"+1"</Button>
        <Input value={email.clone()} on_input={Msg::Email} />
    </Container>
};
```

## Tags

| Tag | Node | Notes |
|---|---|---|
| `Container` | container | Holds children |
| `Heading` | heading | `level` from 1 to 6 |
| `Text` | text | Plain text |
| `Button` | button | `on_click` is required |
| `Input` | input | `value`, and `on_input` for two-way binding |
| `List` | list | Holds items |
| `DataGrid` | data grid | Columns and rows |

## Control flow and components

- `{ if cond { ... } }` and `{ for item in items { ... } }` produce children.
- A component is any expression that returns a `UITree<Msg>`, for example `{ sidebar(&user) }`.
- A subtree that never changes is built once and reused. This happens automatically.

## Compile-time checks

Unknown tags, a `Button` with no `on_click`, and a wrong return type are build errors. The `trybuild`
tests in `crates/tpt-appfront-core/tests/ui` cover these cases.
