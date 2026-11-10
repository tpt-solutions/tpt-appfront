# tpt-appfront-tui

The terminal backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

It maps a `UITree<Msg>` to `ratatui` widgets and reads keyboard input through
`crossterm`. The same `UITree` that drives a web app or a native window can drive a
terminal app.

## Public API

- `run(build_ui, on_event) -> anyhow::Result<()>`: takes over the terminal (raw mode
  and the alternate screen), redraws each frame, and passes each dispatched `Msg` to
  `on_event`. `build_ui` is `Fn() -> UITree<Msg>`.
- `run_with(build_ui, on_event, auto_optimize: bool)`: same as `run`, with automatic
  windowing of large lists.
- `TuiDriver<Msg>`: keyboard focus and dispatch, separate from the terminal.
  `TuiDriver::new(&ui)`, `on_key(KeyEvent) -> Vec<Msg>`, `focus_id()`, `inputs()`,
  `checks()`, `selections()`, `quit()` and `auto_optimize(bool)`.
- `render(&ui, frame, area, ...)`: draws a `UITree` into a ratatui `Frame`.
- `render_to_buffer(&ui, width, height) -> Buffer` and `buffer_to_string(&Buffer)`:
  render off screen for snapshot tests.

## Features

- **Widget mapping.** `Container` becomes a layout split. `Text` and `Heading` become
  paragraphs. `Button` becomes a focusable bracketed paragraph. `Input` and `Textarea`
  become editable text. `Checkbox`, `Select` and `Radio` are focusable and cycle or
  toggle their value. `List` becomes a ratatui list and `DataGrid` a table.
- **Keyboard navigation.** Tab, Shift-Tab and the arrow keys move focus, except on
  `Select` and `Radio`, where the arrows cycle the option. Enter and Space activate
  or toggle. Esc quits. Typing edits the focused text field.
- **Headless tests.** `render_to_buffer` and `TuiDriver::on_key` run without a TTY.
- **Virtual scrolling.** Large `List` and `DataGrid` nodes can be windowed, either
  with `NodeMeta::virtual_scroll` or automatically with `auto_optimize`.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-tui = "0.1"
anyhow = "1"
```

## Example

```rust
use tpt_appfront_core::UITree;

#[derive(Clone, Debug)]
enum Msg {
    Increment,
}

fn main() -> anyhow::Result<()> {
    let build_ui = || -> UITree<Msg> {
        UITree::container(|c| {
            c.heading(1, "Counter");
            c.button("+1").on_click(Msg::Increment);
        })
    };
    tpt_appfront_tui::run(build_ui, |msg| {
        // Update app state here. Printing would corrupt the terminal UI.
        let _ = msg;
    })
}
```

Scaffold a project with `tpt-appfront init --target tui`, or run one with
`tpt-appfront dev --tui`. See `examples/counter-tui`.

## License

MIT OR Apache-2.0
