# Changelog

All notable changes to `tpt-appfront-tui` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `WebviewOptions::allowed_actions` IPC allowlisting and per-second command rate
  limiting (`max_commands_per_second`) to prevent an open-bridge vulnerability.
- IPC message size cap (`MAX_IPC_MESSAGE_BYTES`, 16 KiB) enforced before parsing.
- CLI `--target webview` / `dev --desktop-webview` wiring.

## [0.1.0]

### Added
- Initial release: `wry` + `tao` desktop shell hosting `tpt-appfront-dom`'s `trunk`
  build over an `app://` protocol, allowlisted IPC bridge (`on_command`), and
  `window.__appfront.post(action, params)` for custom JS.
