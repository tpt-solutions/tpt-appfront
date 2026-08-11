# Changelog

All notable changes to `tpt-appfront-tui` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Keyboard-driven event dispatch (`TuiDriver::on_key`): Tab/↑←/↓→ move focus,
  Enter/Space activate, Esc quits, typing edits the focused `Input`.
- Headless rendering via `ratatui`'s `TestBackend` (`render`/`render_to_buffer`/
  `buffer_to_string`).
- CLI `--target tui` / `dev --tui` wiring; `examples/counter-tui`.

## [0.1.0]

### Added
- Initial release: `NodeKind` → `ratatui` widget mapping (`Container`, `Text`,
  `Heading`, `Button`, `Input`, `List`, `DataGrid`), `run` loop over crossterm
  events, and `UITree`-driven terminal apps.
