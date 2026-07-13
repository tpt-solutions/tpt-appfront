# appfront-tui

ratatui terminal backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders an `appfront-core::UITree<Msg>` to `ratatui` widgets: `Container` becomes a layout split, `Text`/`Heading` become paragraphs, `Button` a focusable bracketed paragraph, `Input` an editable line, `List` a list, and `DataGrid` a table. `TuiDriver` handles keyboard-driven focus and dispatch (Tab/arrows/Enter/Space/Esc/typing) independent of any real TTY, and the whole crate is headless-testable via `ratatui`'s `TestBackend`.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture, and `examples/counter-tui` for a runnable terminal example.
