# tpt-appfront-templates

Backend-agnostic starter UI templates for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Each template is a plain `(config, callbacks) -> UITree<Msg>` builder, so the same
tree renders identically on the DOM, canvas, TUI, and HTML backends. They give a
new developer a "real" UI shape to start from — a login form, a nav + content
layout, a CRUD list — instead of the bare counter that `tpt-appfront init`
scaffolds. `Msg` is the app's own event enum; templates take closures that map
template interactions to `Msg` values, keeping them decoupled from any specific
app.

## Templates

- **`login_form`** — a username + password form with a submit `on_submit`.
- **`dashboard_shell`** — a sidebar nav + a `content` region (fill it with another
  template via the `content` callback).
- **`settings_list`** — a CRUD list where each row is its own `Container` (so
  Edit/Delete buttons have somewhere to live), keyed for reconciliation.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-templates = "0.1"
```

## Example

```rust
use tpt_appfront_core::UITree;
use tpt_appfront_templates::{dashboard_shell, settings_list,
    DashboardShellConfig, SettingsListConfig};

let ui: UITree<Msg> = dashboard_shell(&DashboardShellConfig {
    title: "App".into(),
    nav_items: vec!["Home".into(), "Settings".into()],
    content: Box::new(|c| {
        c.with(settings_list(&SettingsListConfig {
            title: "Settings".into(),
            rows: vec![("1".into(), "First".into())],
            on_edit: Box::new(Msg::Edit),
            on_delete: Box::new(Msg::Delete),
        }));
    }),
    on_nav: Box::new(Msg::Nav),
});
```

`examples/templates-demo` composes all three into one runnable DOM app.
`tpt-appfront init --preset <name>` scaffolds projects built on these.

## License

MIT OR Apache-2.0
