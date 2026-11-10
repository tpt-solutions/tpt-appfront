//! Backend-agnostic starter UI templates for TPT AppFront.
//!
//! Each template is a plain `(config, callbacks) -> UITree<Msg>` builder so the
//! same tree renders identically on the DOM, canvas, TUI, and HTML backends.
//! They give a new developer a "real" UI shape to start from (a login form, a
//! nav + content layout, a CRUD list) instead of the bare counter that
//! `tpt-appfront init` scaffolds — see `todo.md` Phase 16.
//!
//! `Msg` is the app's own event enum; templates are generic over it and take
//! closures that map template interactions to the app's `Msg` values. This
//! keeps the templates decoupled from any specific app.

use std::sync::Arc;

use tpt_appfront_core::{ContainerBuilder, UITree};

/// A callback that composes content into an existing `ContainerBuilder`. Used by
/// [`DashboardShellConfig::content`] so callers can nest arbitrary trees
/// (often another template) into a template's content region.
type ContentBuilder<Msg> = Box<dyn Fn(&mut ContainerBuilder<Msg>)>;

/// Which field of a [`login_form`] changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    /// The username input.
    Username,
    /// The password input.
    Password,
}

/// Configuration for [`login_form`]: a username + password prompt and a
/// submit action.
///
/// The template is stateless, so the app owns the field values. Keep them in
/// your own state (for example two `Signal`s), pass the current values in
/// `username` and `password` each time you build the form, and map
/// `on_input` to messages that update that state.
pub struct LoginFormConfig<Msg> {
    /// Title shown above the form.
    pub title: String,
    /// Current username value.
    pub username: String,
    /// Current password value. The input is rendered with `type="password"`.
    pub password: String,
    /// Called with the field and its new value whenever a field changes.
    pub on_input: Arc<dyn Fn(LoginField, String) -> Msg + Send + Sync>,
    /// Message sent when the "Sign in" button is clicked. The app reads the
    /// credentials from the state it kept via `on_input`.
    pub on_submit: Msg,
}

/// A two-field login form (username + password) with a submit button. Rendered
/// as a vertical `Container` of inputs + a `Button`. Both inputs report edits
/// through `on_input`, and the button sends `on_submit`.
pub fn login_form<Msg: Clone + 'static>(cfg: &LoginFormConfig<Msg>) -> UITree<Msg> {
    let on_input = cfg.on_input.clone();
    let on_input_username = on_input.clone();
    let mut b = ContainerBuilder::new();
    b.container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, cfg.title.clone())
            .class("text-2xl font-bold mb-4");
        c.input(cfg.username.clone())
            .class("mb-2 w-full")
            .attr("placeholder", "Username")
            .on_input(move |v| on_input_username(LoginField::Username, v));
        c.input(cfg.password.clone())
            .class("mb-4 w-full")
            .attr("placeholder", "Password")
            .attr("type", "password")
            .on_input(move |v| on_input(LoginField::Password, v));
        c.button("Sign in")
            .class("bg-blue-500 text-white px-4 py-2 rounded")
            .on_click(cfg.on_submit.clone());
    })
    .class("flex flex-col max-w-sm mx-auto mt-12");
    b.into_only_child().unwrap()
}

/// Configuration for [`dashboard_shell`]: a sidebar nav plus a content area
/// that the caller fills.
pub struct DashboardShellConfig<Msg> {
    /// App / brand title shown at the top of the sidebar.
    pub title: String,
    /// Nav item labels. Selecting one produces a `Msg` via [`DashboardShellConfig::on_nav`].
    pub nav_items: Vec<String>,
    /// Fills the main content area. Receives a `ContainerBuilder` so callers
    /// compose arbitrary content (often a nested template like [`settings_list`]).
    pub content: ContentBuilder<Msg>,
    /// Maps a clicked nav item (by its label) to a `Msg`.
    pub on_nav: Box<dyn Fn(String) -> Msg>,
}

/// A nav + content dashboard layout: a left sidebar of nav buttons and a main
/// content region. The sidebar uses `Container` with `class` for column
/// layout; the content is whatever `content` builds.
pub fn dashboard_shell<Msg: Clone + 'static>(cfg: &DashboardShellConfig<Msg>) -> UITree<Msg> {
    let on_nav = &cfg.on_nav;
    let content = &cfg.content;
    let mut b = ContainerBuilder::new();
    b.container(|c: &mut ContainerBuilder<Msg>| {
        c.container(|sidebar: &mut ContainerBuilder<Msg>| {
            sidebar
                .heading(2, cfg.title.clone())
                .class("text-xl font-semibold mb-4");
            for item in &cfg.nav_items {
                let label = item.clone();
                sidebar
                    .button(item.clone())
                    .class("block w-full text-left py-2 px-3 rounded hover:bg-gray-100")
                    .on_click((on_nav)(label));
            }
        })
        .class("w-64 h-full bg-gray-50 p-4 border-r");
        c.container(|main: &mut ContainerBuilder<Msg>| {
            content(main);
        })
        .class("flex-1 p-6 overflow-auto");
    })
    .class("flex h-screen");
    b.into_only_child().unwrap()
}

/// Configuration for [`settings_list`]: a CRUD list of rows, each with an
/// Edit / Delete button.
pub struct SettingsListConfig<Msg> {
    /// Title shown above the list.
    pub title: String,
    /// The list rows. `id` is a stable key for reconciliation; `label` is shown.
    pub rows: Vec<(String, String)>,
    /// Maps a row id to a `Msg` when its Edit button is clicked.
    pub on_edit: Box<dyn Fn(String) -> Msg>,
    /// Maps a row id to a `Msg` when its Delete button is clicked.
    pub on_delete: Box<dyn Fn(String) -> Msg>,
}

/// A CRUD list: each row is its own `Container` (so Edit/Delete buttons have
/// somewhere to live) with a label and two action buttons. Uses `key` for
/// keyed reconciliation when rows are added/removed/reordered.
pub fn settings_list<Msg: Clone + 'static>(cfg: &SettingsListConfig<Msg>) -> UITree<Msg> {
    let on_edit = &cfg.on_edit;
    let on_delete = &cfg.on_delete;
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, cfg.title.clone())
            .class("text-2xl font-bold mb-4");
        c.list(|items: &mut ContainerBuilder<Msg>| {
            for (id, label) in &cfg.rows {
                let row_id = id.clone();
                let row_label = label.clone();
                items
                    .container(|row: &mut ContainerBuilder<Msg>| {
                        row.text(row_label).class("flex-1");
                        row.button("Edit")
                            .class("mr-2 px-3 py-1 rounded border")
                            .on_click((on_edit)(row_id.clone()));
                        row.button("Delete")
                            .class("px-3 py-1 rounded border text-red-600")
                            .on_click((on_delete)(row_id.clone()));
                    })
                    .class("flex items-center py-2 border-b")
                    .key(id.clone());
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use tpt_appfront_core::NodeKind;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Submitted,
        Field(LoginField, String),
        Nav(String),
        Edit(String),
        Delete(String),
    }

    fn count_buttons(ui: &UITree<Msg>) -> usize {
        let mut n = 0;
        fn walk<M>(ui: &UITree<M>, n: &mut usize) {
            if let NodeKind::Button { .. } = &ui.kind {
                *n += 1;
            }
            if let NodeKind::Container { children } = &ui.kind {
                for c in children {
                    walk(c, n);
                }
            }
            if let NodeKind::List { items } = &ui.kind {
                for c in items {
                    walk(c, n);
                }
            }
        }
        walk(ui, &mut n);
        n
    }

    #[test]
    fn login_form_has_submit_button() {
        let form = login_form(&LoginFormConfig {
            title: "Sign in".into(),
            username: String::new(),
            password: String::new(),
            on_input: Arc::new(|_, _| Msg::Submitted),
            on_submit: Msg::Submitted,
        });
        assert_eq!(count_buttons(&form), 1);
    }

    #[test]
    fn login_form_reports_password_input_and_submits_plain_message() {
        let form = login_form(&LoginFormConfig {
            title: "Sign in".into(),
            username: "ada".into(),
            password: "hunter2".into(),
            on_input: Arc::new(Msg::Field),
            on_submit: Msg::Submitted,
        });
        let mut found_password: Option<Msg> = None;
        fn walk(ui: &UITree<Msg>, out: &mut Option<Msg>) {
            if let NodeKind::Input { value } = &ui.kind {
                if value == "hunter2" {
                    *out = ui.meta.on_input.as_ref().map(|f| f("x".into()));
                }
            }
            if let NodeKind::Container { children } = &ui.kind {
                for c in children {
                    walk(c, out);
                }
            }
        }
        walk(&form, &mut found_password);
        assert_eq!(
            found_password,
            Some(Msg::Field(LoginField::Password, "x".into())),
            "password input must report its edits as LoginField::Password"
        );
    }

    #[test]
    fn dashboard_shell_composes_settings_list_into_content() {
        let shell = dashboard_shell(&DashboardShellConfig {
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
        // 2 nav buttons + 2 (Edit/Delete) from the composed settings list
        assert_eq!(count_buttons(&shell), 4);
    }

    #[test]
    fn settings_list_builds_edit_and_delete_per_row() {
        let list = settings_list(&SettingsListConfig {
            title: "Settings".into(),
            rows: vec![("1".into(), "First".into()), ("2".into(), "Second".into())],
            on_edit: Box::new(Msg::Edit),
            on_delete: Box::new(Msg::Delete),
        });
        // 2 rows * (Edit + Delete) = 4 buttons
        assert_eq!(count_buttons(&list), 4);
    }
}
