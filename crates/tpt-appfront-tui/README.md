# tpt-appfront-tui

The terminal UI backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Proves the "one `UITree`, N renderers" thesis one step further: the same
`UITree<Msg>` that drives a web app, a native window, or an AI agent also drives a
terminal app — a claim no DOM-rooted framework (Leptos/Dioxus/Yew) can make
without a rewrite of their AST.

## Features

- **`NodeKind` → `ratatui` widget** mapping (`Container` → layout split,
  `Text`/`Heading` → paragraph, `Button` → focusable paragraph, `Input` → editable
  line, `List` → `ratatui` list, `DataGrid` → `ratatui` table).
- **Keyboard-driven dispatch** — Tab/↑←/↓→ move focus, Enter/Space activate the
  focused button, Esc quits, typing edits the focused `Input`. Mirrors the DOM/
  canvas dispatch closure pattern.
- **Headless-testable** — rendering is pure; `render`/`render_to_buffer`/
  `buffer_to_string` work against `ratatui`'s `TestBackend`, so keyboard handling
  (`TuiDriver::on_key`) is unit-testable without a TTY.
- **`run`** — the real-terminal entry point (crossterm raw mode + alternate screen).

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-tui = "0.1"
```

## Example

```rust
use tpt_appfront_core::{Signal, UITree};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = Signal::new(0i32);
    let build_ui = move || -> UITree<Msg> {
        UITree::container(|c| {
            c.heading(1, "Counter");
            c.button("+1").on_click(Msg::Increment);
        })
    };
    tpt_appfront_tui::run(build_ui, |msg| { /* dispatch */ })
}
```

## License

MIT OR Apache-2.0
