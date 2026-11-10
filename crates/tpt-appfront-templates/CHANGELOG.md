# Changelog

All notable changes to `tpt-appfront-templates` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Starter presets in the CLI (`tpt-appfront init --preset login|dashboard|crud-app|
  saas-starter`) are built on these templates, wired to live `Signal`-backed state.

### Changed
- **Breaking:** `LoginFormConfig` has `password` and `on_input` fields
  (`on_input: Arc<dyn Fn(LoginField, String) -> Msg + Send + Sync>`), and `on_submit` is now a
  plain `Msg` instead of `Box<dyn Fn(String, String) -> Msg>`. The new `LoginField` enum
  tells `on_input` which field changed.

### Fixed
- `login_form` now reads the password. Its inputs report edits through `on_input`. The
  "Sign in" message no longer carries a build-time username with an empty password. The app
  keeps the values in its own state and passes them back in.

## [0.1.0]

### Added
- Initial release: backend-agnostic starter UIs — `login_form`, `dashboard_shell`, and
  `settings_list` — as stateless `(config, callbacks) -> UITree<Msg>` builders usable
  from any backend.
