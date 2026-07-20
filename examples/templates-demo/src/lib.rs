//! Demo wiring the three backend-agnostic `tpt-appfront-templates` starter
//! templates (`login_form`, `dashboard_shell`, `settings_list`) into one
//! runnable DOM app, proving template *composition* works end to end.
//!
//! `dashboard_shell`'s `content` is filled with `settings_list`'s output, just
//! like `todo.md` Phase 16 describes. The app is interactive: editing a row's
//! text, signing in, and navigating the sidebar all dispatch `Msg`s that mutate
//! shared `Signal` state.

use tpt_appfront_core::{create_effect, Signal, UITree};
use tpt_appfront_templates::{
    dashboard_shell, login_form, settings_list, DashboardShellConfig, LoginFormConfig,
    SettingsListConfig,
};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {
    Login(String, String),
    Nav(String),
    Edit(String),
    Delete(String),
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let rows = Signal::new(vec![
        ("1".to_string(), "Profile".to_string()),
        ("2".to_string(), "Billing".to_string()),
        ("3".to_string(), "Notifications".to_string()),
    ]);
    let logged_in = Signal::new(false);
    let current_page = Signal::new("Overview".to_string());

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = {
        let rows = rows.clone();
        let logged_in = logged_in.clone();
        let current_page = current_page.clone();
        std::rc::Rc::new(move |msg| match msg {
            Msg::Login(user, _pass) => {
                if !user.is_empty() {
                    logged_in.set(true);
                }
            }
            Msg::Nav(page) => current_page.set(page),
            Msg::Edit(id) => current_page.set(format!("Edit {id}")),
            Msg::Delete(id) => {
                let v: Vec<(String, String)> =
                    rows.get().into_iter().filter(|(r, _)| r != &id).collect();
                rows.set(v);
            }
        })
    };

    // ---- login gate (shown until a username is entered) ----
    let gate = document.create_element("div")?;
    body.append_child(&gate)?;

    let login_ui: UITree<Msg> = login_form(&LoginFormConfig {
        title: "Sign in to AppFront".into(),
        username: String::new(),
        on_submit: Box::new(Msg::Login),
    });
    let gate_handle = tpt_appfront_dom::mount(&gate, &login_ui, dispatch.clone())?;

    // ---- dashboard (the composed templates) ----
    let shell = document.create_element("div")?;
    body.append_child(&shell)?;

    let rows_for_ui = rows.clone();
    let current_page_for_ui = current_page.clone();
    let shell_ui: UITree<Msg> = {
        let rows = rows_for_ui.clone();
        let current_page = current_page_for_ui.clone();
        dashboard_shell(&DashboardShellConfig {
            title: "AppFront".into(),
            nav_items: vec!["Overview".into(), "Settings".into(), "Help".into()],
            content: Box::new(move |c| {
                let rows = rows.clone();
                let current_page = current_page.clone();
                let inner = settings_list(&SettingsListConfig {
                    title: current_page.get(),
                    rows: rows.get(),
                    on_edit: Box::new(Msg::Edit),
                    on_delete: Box::new(Msg::Delete),
                });
                c.with(inner);
            }),
            on_nav: Box::new(Msg::Nav),
        })
    };
    let shell_handle = tpt_appfront_dom::mount(&shell, &shell_ui, dispatch.clone())?;

    let logged_in_for_toggle = logged_in.clone();
    let gate_for_toggle = gate.clone();
    let _toggle = create_effect(move || {
        let show_gate = !logged_in_for_toggle.get();
        gate_for_toggle
            .set_attribute("style", if show_gate { "" } else { "display:none" })
            .ok();
        shell
            .set_attribute("style", if show_gate { "display:none" } else { "" })
            .ok();
    });
    std::mem::forget(_toggle);

    // The two mounted subtrees are whole-process roots; forgetting the handles
    // is the explicit leak choice for a static SPA root mount.
    std::mem::forget(gate_handle);
    std::mem::forget(shell_handle);

    Ok(())
}
