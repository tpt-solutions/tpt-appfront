# tpt-appfront-templates

Backend-agnostic starter UI templates for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Each template is a plain `(config, callbacks) -> UITree<Msg>` function. Templates hold
no state of their own. Pass the current values in the config, and map each interaction
to a `Msg` with a callback. `Msg` is your app's own event enum, so the templates stay
independent of any particular app. Because the output is an ordinary `UITree`, a
template can be rendered by any backend.

They give a new project a realistic starting layout, such as a login form, a nav and
content shell, or a CRUD list, instead of the bare counter that `tpt-appfront init`
scaffolds by default.

## Public API

Re-exported from `src/lib.rs`:

- `login_form<Msg>(&LoginFormConfig<Msg>) -> UITree<Msg>`
  - `LoginFormConfig { title, username, password, on_input, on_submit }`. `username` and
    `password` are the current values, which the app owns. `on_input` is
    `Arc<dyn Fn(LoginField, String) -> Msg + Send + Sync>` and is called on every edit.
    `on_submit` is a plain `Msg`. `LoginField` is `Username` or `Password`.
- `dashboard_shell<Msg>(&DashboardShellConfig<Msg>) -> UITree<Msg>`
  - `DashboardShellConfig { title, nav_items, content, on_nav }`. `content` is
    `Box<dyn Fn(&mut ContainerBuilder<Msg>)>`, which fills the main area, and `on_nav`
    is `Box<dyn Fn(String) -> Msg>`, called with the clicked item's label.
- `settings_list<Msg>(&SettingsListConfig<Msg>) -> UITree<Msg>`
  - `SettingsListConfig { title, rows, on_edit, on_delete }`. `rows` is
    `Vec<(String, String)>` of `(id, label)`. The callbacks receive the row id.

`Msg` must be `Clone + 'static`.

## Templates

- **`login_form`**: a heading, a username input and a password input (`type="password"`),
  and a "Sign in" button. Each input reports its edits through `on_input`. The template
  holds no state, so the app keeps the values (for example in two `Signal`s) and passes
  them back in as `username` and `password`. The button sends `on_submit`, and the app
  reads the credentials from its own state.
- **`dashboard_shell`**: a sidebar with a title and one button per nav item, and a main
  area filled by `content`. Compose another template into `content` with
  `ContainerBuilder::with`.
- **`settings_list`**: a heading and a list where each row is its own container with a
  label, an Edit button and a Delete button. Rows are keyed by id, so reordering them
  reconciles cleanly.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-templates = "0.1"
```

Or enable the `templates` feature of the umbrella crate, `tpt-appfront`.

This crate has no Cargo feature flags.

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
`tpt-appfront init --preset <name>` (`login`, `dashboard`, `crud-app`, `saas-starter`)
and `tpt-appfront init --with <a,b,c>` scaffold projects built on these templates.

## License

MIT OR Apache-2.0
