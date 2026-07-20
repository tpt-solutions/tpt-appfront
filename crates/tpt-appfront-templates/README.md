# tpt-appfront-templates

Backend-agnostic starter UI templates for TPT AppFront. These are plain
`(config, callbacks) -> UITree<Msg>` builders, so the same tree renders
identically on the DOM, canvas, TUI, and HTML backends.

- `login_form` — username + password form with a submit action
- `dashboard_shell` — nav sidebar + content area (compose `settings_list` into the content)
- `settings_list` — CRUD list with per-row Edit/Delete buttons (uses `.key` on each row for keyed reconciliation)

See `docs/quickstart.md` and `examples/templates-demo` for usage.
