# tpt-appfront-tui

Terminal UI backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) (ratatui/crossterm), keyboard-driven focus and dispatch.

Maps `UITree<Msg>` nodes to `ratatui` widgets (containers to layout splits, text/headings to paragraphs, buttons to focusable bracketed paragraphs, inputs to editable lines, lists/grids to list/table widgets) and drives Tab/arrow focus, Enter/Space activation, and Esc-to-quit via `TuiDriver` — headless-testable via `ratatui`'s `TestBackend`.

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-tui = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and the [`counter-tui` example](https://github.com/tpt-solutions/tpt-appfront/tree/main/examples/counter-tui).

## License

MIT OR Apache-2.0
