# Changelog

All notable changes to `tpt-appfront-tui` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Keyboard dispatch through `TuiDriver::on_key`. Tab and Shift-Tab move focus. Up,
  Down, Left and Right move focus, except on `Select` and `Radio`, where they cycle
  the option. Enter and Space activate buttons and toggle checkboxes. Typing edits the
  focused `Input` or `Textarea`. Esc quits.
- `Textarea`, `Checkbox`, `Select` and `Radio` are focusable and interactive.
- `on_focus`, `on_blur` and `on_keydown` handlers are dispatched on focus changes and
  key presses. Content inside a `Portal` is focusable and rendered inline.
- `Image`, `Link` and `Media` nodes render their alt or link text.
- Virtual scrolling for `List` and `DataGrid` nodes. `run_with`, `render_to_buffer_with`
  and `TuiDriver::auto_optimize` window large unconfigured lists automatically.
- Headless rendering: `render`, `render_to_buffer` and `buffer_to_string` work against
  ratatui's `TestBackend`, so keyboard handling can be tested without a terminal.
- CLI `--target tui` and `dev --tui` wiring; `examples/counter-tui`.

## [0.1.0]

### Added
- Initial release: `NodeKind` → `ratatui` widget mapping (`Container`, `Text`,
  `Heading`, `Button`, `Input`, `List`, `DataGrid`), `run` loop over crossterm
  events, and `UITree`-driven terminal apps.
