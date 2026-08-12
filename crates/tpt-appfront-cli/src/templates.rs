//! String templates used by `tpt-appfront init` to scaffold a new project.
//! The dependency spec (`core_dep`/`canvas_dep`/etc.) is computed by the
//! caller: a `path = "..."` dependency when running against the
//! `tpt-appfront` checkout that built this CLI, or a version dependency
//! (`"0.1.0"`) once the crates are published and `tpt-appfront init` is invoked
//! from an installed `cargo install tpt-appfront-cli` (see `dep_ref` in
//! `main.rs`). This avoids scaffolding projects with broken path deps on a
//! published install.

pub fn canvas_cargo_toml(pkg_name: &str, core_dep: &str, canvas_dep: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
tpt-appfront-core = {core_dep}
tpt-appfront-canvas = {canvas_dep}
"#
    )
}

pub fn canvas_main_rs(app_title: &str) -> String {
    format!(
        r#"use tpt_appfront_core::{{Signal, UITree}};

#[derive(Debug, Clone)]
enum Msg {{
    Increment,
}}

fn main() -> Result<(), Box<dyn std::error::Error>> {{
    let count = Signal::new(0i32);

    let count_for_ui = count.clone();
    let build_ui = move || -> UITree<Msg> {{
        UITree::container(|c| {{
            c.heading(1, "{app_title}");
            c.text(format!("Count: {{}}", count_for_ui.get()));
            c.button("+1").on_click(Msg::Increment);
        }})
    }};

    let dispatch = move |msg: Msg| match msg {{
        Msg::Increment => count.set(count.get() + 1),
    }};

    // Devtools inspection: when `TPT_APPFRONT_DEVTOOLS=1` (set by
    // `tpt-appfront dev --devtools`), print the `UITree` structure on startup.
    if std::env::var("TPT_APPFRONT_DEVTOOLS").is_ok() {{
        let preview = build_ui();
        eprintln!("{{}}", tpt_appfront_core::devtools::inspect_tree(&preview));
    }}

    tpt_appfront_canvas::run_native("{app_title}", build_ui, dispatch)?;
    Ok(())
}}
"#
    )
}

pub fn dom_cargo_toml(pkg_name: &str, core_dep: &str, dom_dep: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
tpt-appfront-core = {core_dep}
tpt-appfront-dom = {dom_dep}
wasm-bindgen = "0.2"
web-sys = {{ version = "0.3", features = ["Document", "Window", "Element"] }}
console_error_panic_hook = "0.1"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
"#
    )
}

pub fn dom_lib_rs(app_title: &str) -> String {
    format!(
        r#"use tpt_appfront_core::{{create_effect, Signal, UITree}};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {{
    Increment,
}}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {{
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let container = document.create_element("div")?;
    body.append_child(&container)?;

    let count = Signal::new(0i32);
    let display = Signal::new(format!("Count: {{}}", count.get()));

    let count_for_effect = count.clone();
    let display_for_effect = display.clone();
    let handle = create_effect(move || {{
        display_for_effect.set(format!("Count: {{}}", count_for_effect.get()));
    }});
    std::mem::forget(handle);

    let ui: UITree<Msg> = UITree::container(|c| {{
        c.heading(1, "{app_title}");
        c.button("+1").on_click(Msg::Increment);
    }});

    let count_for_dispatch = count.clone();
    let dispatch: Rc<dyn Fn(Msg)> = Rc::new(move |msg| match msg {{
        Msg::Increment => count_for_dispatch.set(count_for_dispatch.get() + 1),
    }});

    // Devtools inspection: when `TPT_APPFRONT_DEVTOOLS=1` (set by
    // `tpt-appfront dev --devtools`), print the `UITree` structure on startup.
    if std::env::var("TPT_APPFRONT_DEVTOOLS").is_ok() {{
        eprintln!("{{}}", tpt_appfront_core::devtools::inspect_tree(&ui));
    }}

    tpt_appfront_dom::mount(&container, &ui, dispatch)?;

    let (text_node, text_handle) = tpt_appfront_dom::reactive_text(&document, display)?;
    container.append_child(&text_node)?;
    // Whole-process root mount: forgetting is an explicit choice here, not
    // reactive_text's default behavior.
    std::mem::forget(text_handle);

    Ok(())
}}
"#
    )
}

pub fn index_html(app_title: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>{app_title}</title>
    <!-- data-wasm-opt runs Binaryen wasm-opt (-Oz) on the built wasm during `trunk build --release` to trim payload size -->
    <link data-trunk rel="rust" href="Cargo.toml" data-wasm-opt="z" />
  </head>
  <body></body>
</html>
"#
    )
}

pub fn tui_cargo_toml(pkg_name: &str, core_dep: &str, tui_dep: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
tpt-appfront-core = {core_dep}
tpt-appfront-tui = {tui_dep}
"#
    )
}

pub fn tui_main_rs(app_title: &str) -> String {
    format!(
        r#"use tpt_appfront_core::{{Signal, UITree}};

#[derive(Debug, Clone)]
enum Msg {{
    Increment,
}}

fn main() -> Result<(), Box<dyn std::error::Error>> {{
    let count = Signal::new(0i32);

    let count_for_ui = count.clone();
    let build_ui = move || -> UITree<Msg> {{
        UITree::container(|c| {{
            c.heading(1, "{app_title}");
            c.text(format!("Count: {{}}", count_for_ui.get()));
            c.button("+1").on_click(Msg::Increment);
        }})
    }};

    let dispatch = move |msg: Msg| match msg {{
        Msg::Increment => count.set(count.get() + 1),
    }};

    // Devtools inspection: when `TPT_APPFRONT_DEVTOOLS=1` (set by
    // `tpt-appfront dev --devtools`), print the `UITree` structure on startup.
    if std::env::var("TPT_APPFRONT_DEVTOOLS").is_ok() {{
        let preview = build_ui();
        eprintln!("{{}}", tpt_appfront_core::devtools::inspect_tree(&preview));
    }}

    // Tab/Arrows move focus, Enter/Space activate, Esc quits.
    tpt_appfront_tui::run(build_ui, dispatch)?;
    Ok(())
}}
"#
    )
}

pub fn server_cargo_toml(pkg_name: &str, core_dep: &str, server_dep: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
tpt-appfront-core = {core_dep}
tpt-appfront-server = {server_dep}
tokio = {{ version = "1", features = ["full"] }}
"#
    )
}

/// `main.rs` for `init --server`: a standalone Axum smart-router server that
/// serves the same `UITree` to humans (WASM shell), crawlers (semantic HTML),
/// AI agents (JSON-LD + AI Schema) and social bots (OpenGraph) — wiring
/// `tpt-appfront-server` directly instead of standing it up by hand (todo.md
/// Phase 20 missing-feature follow-up).
pub fn server_main_rs(app_title: &str) -> String {
    format!(
        r#"use tpt_appfront_core::UITree;
use tpt_appfront_server::{{SmartRouterBuilder, serve}};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {{
    let ui: UITree<()> = UITree::container(|c| {{
        c.heading(1, "{app_title}");
        c.text("Served by the TPT AppFront smart router.");
        c.button("Get started").ai_action("get_started");
    }});

    let router = SmartRouterBuilder::new(ui)
        .title("{app_title}")
        .description("A TPT AppFront smart-router demo server.")
        .build();

    let addr: std::net::SocketAddr = "127.0.0.1:3000".parse()?;
    println!("Serving on http://{{addr}}");
    serve(router, addr).await;
    Ok(())
}}
"#
    )
}

pub fn gitignore() -> &'static str {
    "/target\n/dist\nCargo.lock\n"
}

/// `cargo-packager` config (`packager.toml`) written by `tpt-appfront build
/// --bundle` / `tpt-appfront optimize --bundle` when one isn't already present.
/// Produces per-OS installers (.msi/.dmg/.appimage/.deb) plus delta auto-update
/// artifacts (todo.md Phase 11 stretch). Tune `formats`/signing to taste.
pub fn packager_toml(pkg_name: &str) -> String {
    format!(
        r#"[package]
product-name = "{pkg_name}"
version = "0.1.0"

[packager]
# Installer/archive formats per target OS:
#   windows -> msi, nsis
#   macos   -> app, dmg
#   linux   -> appimage, deb
formats = ["msi", "dmg", "appimage", "deb"]
# Emit delta auto-update artifacts alongside the installers.
generate-updates = true
"#
    )
}

pub fn readme(name: &str, both: bool) -> String {
    if both {
        format!(
            r#"# {name}

Scaffolded by `tpt-appfront init`.

- `canvas/` — native desktop app (winit/egui via `tpt-appfront-canvas`). Run with:
  ```sh
  cd canvas && cargo run
  ```
- `dom/` — browser app (real DOM via `tpt-appfront-dom`). Run with:
  ```sh
  cd dom && trunk serve
  ```

Or drive both through the CLI from this directory:
```sh
tpt-appfront dev --desktop --project canvas
tpt-appfront dev --web --project dom
tpt-appfront build --target canvas --project canvas
tpt-appfront build --target dom --project dom
```
"#
        )
    } else {
        format!(
            r#"# {name}

Scaffolded by `tpt-appfront init`. See `tpt-appfront dev --help` / `tpt-appfront build --help`.
"#
        )
    }
}

/// Skeleton for `tpt-appfront add component <name>`: a `view!`-based reusable
/// UI fragment that takes a `Msg` dispatch closure and returns a `UITree<Msg>`.
/// The component name is kebab-cased into the root node's class.
pub fn component_rs(name: &str, kebab: &str) -> String {
    format!(
        r#"//! Component `{name}` — scaffolded by `tpt-appfront add component`.

use tpt_appfront_core::{{UITree, view}};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Msg {{
    Activated,
}}

/// Builds the component's `UITree`. Wire `dispatch` into your app's `Msg`.
pub fn {name}() -> UITree<Msg> {{
    let dispatch: Rc<dyn Fn(Msg)> = Rc::new(|msg| match msg {{
        Msg::Activated => {{ /* TODO: handle activation */ }}
    }});
    view! {{
        <Container class="{kebab}">
            <Heading level={{1u8}}>"{name}"</Heading>
            <Button on_click={{ move |_| dispatch(Msg::Activated) }}>"Activate"</Button>
        </Container>
    }}
}}
"#
    )
}

/// Skeleton for `tpt-appfront add page <name>`: a route-sized `view!` screen
/// that fills the content area of a `dashboard_shell`/`AppBuilder` host.
pub fn page_rs(name: &str, kebab: &str) -> String {
    format!(
        r#"//! Page `{name}` — scaffolded by `tpt-appfront add page`.

use tpt_appfront_core::{{UITree, view}};

#[derive(Debug, Clone)]
pub enum Msg {{
    Navigated,
}}

/// Builds the page's `UITree` for route `{kebab}`.
pub fn {name}() -> UITree<Msg> {{
    view! {{
        <Container class="{kebab}">
            <Heading level={{1u8}}>"{name}"</Heading>
            <Text>"This page was scaffolded by `tpt-appfront add page`."</Text>
        </Container>
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// presets (init --preset <name>)
// ---------------------------------------------------------------------------

/// `Cargo.toml` for a preset-scaffolded DOM app. Adds `tpt-appfront-templates`
/// on top of the usual `tpt-appfront-core`/`tpt-appfront-dom` deps, and the
/// `web-sys` features the starter templates read (inputs, elements, nodes).
pub fn preset_cargo_toml(
    pkg_name: &str,
    core_dep: &str,
    dom_dep: &str,
    templates_dep: &str,
) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
tpt-appfront-core = {core_dep}
tpt-appfront-dom = {dom_dep}
tpt-appfront-templates = {templates_dep}
wasm-bindgen = "0.2"
web-sys = {{ version = "0.3", features = ["Document", "Window", "Element", "HtmlInputElement", "HtmlElement", "Node"] }}
console_error_panic_hook = "0.1"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
"#
    )
}

/// `lib.rs` for a preset-scaffolded DOM app. Each preset wires the matching
/// `tpt_appfront_templates` builder (or, for `login`, a `view!` form) to real
/// `Signal`-backed state, so the scaffolded app is interactive out of the box.
pub fn preset_lib_rs(preset: &crate::presets::Preset, app_title: &str) -> String {
    match preset {
        crate::presets::Preset::Login => login_preset_lib_rs(app_title),
        crate::presets::Preset::Dashboard => dashboard_preset_lib_rs(app_title),
        crate::presets::Preset::CrudApp => crud_app_preset_lib_rs(app_title),
        crate::presets::Preset::SaasStarter => saas_preset_lib_rs(app_title),
    }
}

/// `lib.rs` for an `init --with <a,b,c>` scaffold. Like the presets it's a DOM
/// app that composes selected `tpt_appfront_templates` pieces into one project:
/// a `dashboard` piece hosts the other selected pieces in its content area;
/// without `dashboard`, the selected pieces stack vertically. All pieces share
/// one `Signal`-backed state and a single `Msg` union (`todo.md` Phase cross-cutting).
pub fn with_lib_rs(pieces: &[crate::presets::WithPiece], app_title: &str) -> String {
    use crate::presets::WithPiece;
    let has_dashboard = pieces.contains(&WithPiece::Dashboard);
    let has_login = pieces.contains(&WithPiece::Login);
    let has_settings = pieces.contains(&WithPiece::Settings);

    // Only pull in the `tpt_appfront_templates` pieces actually selected, so a
    // generated project has no unused-import warnings.
    let mut imports = String::new();
    if has_dashboard {
        imports.push_str("    dashboard_shell, DashboardShellConfig,\n");
    }
    if has_settings {
        imports.push_str("    settings_list, SettingsListConfig,\n");
    }
    if has_login {
        imports.push_str("    login_form, LoginFormConfig,\n");
    }
    // The imported symbols above are always used (each selected piece emits a
    // `c.with(piece(&PieceConfig { .. }))` call below), so the trailing comma
    // after the last one is fine.

    // Each selected piece's contribution to the tree (dashboard content area,
    // or the stacked container when `dashboard` isn't selected).
    let mut content_calls = String::new();
    if has_settings {
        content_calls.push_str(
            r#"
            c.with(settings_list(&SettingsListConfig {
                title: "Settings".to_string(),
                rows: rows_for_ui.get(),
                on_edit: Box::new(Msg::Edit),
                on_delete: Box::new(Msg::Delete),
            }));"#,
        );
    }
    if has_login {
        content_calls.push_str(
            r#"
            c.with(login_form(&LoginFormConfig {
                title: "Sign in".to_string(),
                username: username_for_ui.get(),
                on_submit: Box::new(|u, p| Msg::Submit(u, p)),
            }));"#,
        );
    }

    // Only declare the state clones each selected piece actually uses, so a
    // generated project stays warning-free.
    let mut content_lets = String::new();
    if has_settings || has_login {
        content_lets.push_str("\n            let rows_for_ui = rows_for_ui.clone();");
    }
    if has_login {
        content_lets.push_str("\n            let username_for_ui = username_for_ui.clone();");
    }

    let ui_build = if has_dashboard {
        format!(
            r#"dashboard_shell(&DashboardShellConfig {{
        title: "{app_title}".to_string(),
        nav_items: vec!["Overview".to_string(), "Settings".to_string(), "Help".to_string()],
        content: Box::new(move |c| {{{content_lets}
{content_calls}
        }}),
        on_nav: Box::new(Msg::Nav),
    }})"#
        )
    } else {
        format!(
            r#"UITree::container(|c| {{{content_calls}
    }})"#
        )
    };

    format!(
        r#"//! Composed starter — scaffolded by `tpt-appfront init --with {pieces_csv}`.
//! Combines the requested `tpt_appfront_templates` pieces ({pieces_list}) into a
//! single DOM app wired to shared `Signal`-backed state.

use tpt_appfront_core::{{Signal, UITree}};
use tpt_appfront_templates::{{
{imports}}};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {{
    Nav(String),
    Edit(String),
    Delete(String),
    Submit(String, String),
}}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {{
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let rows = Signal::new(vec![
        ("1".to_string(), "Profile".to_string()),
        ("2".to_string(), "Billing".to_string()),
        ("3".to_string(), "Notifications".to_string()),
    ]);
    let username = Signal::new(String::new());
    let password = Signal::new(String::new());

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = {{
        let rows = rows.clone();
        std::rc::Rc::new(move |msg| match msg {{
            Msg::Nav(page) => {{ let _ = page; }}
            Msg::Edit(id) => {{
                let v: Vec<(String, String)> =
                    rows.get().into_iter().filter(|(r, _)| r != &id).collect();
                rows.set(v);
            }}
            Msg::Delete(id) => {{
                let v: Vec<(String, String)> =
                    rows.get().into_iter().filter(|(r, _)| r != &id).collect();
                rows.set(v);
            }}
            Msg::Submit(_u, _p) => {{}}
        }})
    }};

    let root = document.create_element("div")?;
    body.append_child(&root)?;

    let rows_for_ui = rows.clone();
    let username_for_ui = username.clone();
    let ui: UITree<Msg> = {ui_build};

    let handle = tpt_appfront_dom::mount(&root, &ui, dispatch)?;
    std::mem::forget(handle);
    Ok(())
}}
"#,
        pieces_csv = pieces
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
            .join(","),
        pieces_list = pieces
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn login_preset_lib_rs(app_title: &str) -> String {
    format!(
        r#"//! Login preset — scaffolded by `tpt-appfront init --preset login`.
//! A sign-in form with live, `Signal`-bound username/password fields. Typed
//! values flow into `Msg`s that mutate shared `Signal` state; the template
//! demonstrates the `view!` two-way-binding path (`on_input={{Msg::Set..}}`).

use tpt_appfront_core::{{Signal, UITree, view}};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {{
    SetUsername(String),
    SetPassword(String),
    Submit,
}}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {{
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let username = Signal::new(String::new());
    let password = Signal::new(String::new());
    let submitted = Signal::new(false);

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = {{
        let username = username.clone();
        let password = password.clone();
        let submitted = submitted.clone();
        std::rc::Rc::new(move |msg| match msg {{
            Msg::SetUsername(v) => username.set(v),
            Msg::SetPassword(v) => password.set(v),
            Msg::Submit => {{
                if !username.get().is_empty() && !password.get().is_empty() {{
                    submitted.set(true);
                }}
            }}
        }})
    }};

    let username_for_ui = username.clone();
    let password_for_ui = password.clone();
    let submitted_for_ui = submitted.clone();
    let ui: UITree<Msg> = view! {{
        <Container class="login">
            <Heading level={{1u8}}>"{app_title}"</Heading>
            <Input value={{username_for_ui.get()}} on_input={{Msg::SetUsername}} />
            <Input value={{password_for_ui.get()}} on_input={{Msg::SetPassword}} />
            <Button on_click={{Msg::Submit}}>"Sign in"</Button>
            {{if submitted_for_ui.get() {{
                <Text>"Welcome!"</Text>
            }} else {{
                <Text>"Enter your credentials."</Text>
            }}}}
        </Container>
    }};

    let root = document.create_element("div")?;
    body.append_child(&root)?;
    let handle = tpt_appfront_dom::mount(&root, &ui, dispatch)?;
    std::mem::forget(handle);
    Ok(())
}}
"#
    )
}

fn dashboard_preset_lib_rs(app_title: &str) -> String {
    format!(
        r#"//! Dashboard preset — scaffolded by `tpt-appfront init --preset dashboard`.
//! A nav sidebar (`dashboard_shell`) whose content area composes a CRUD list
//! (`settings_list`), both from `tpt_appfront_templates`, wired to `Signal`
//! state. Navigating the sidebar and editing/deleting rows dispatch `Msg`s.

use tpt_appfront_core::{{Signal, UITree}};
use tpt_appfront_templates::{{
    dashboard_shell, settings_list, DashboardShellConfig, SettingsListConfig,
}};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {{
    Nav(String),
    Edit(String),
    Delete(String),
}}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {{
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let rows = Signal::new(vec![
        ("1".to_string(), "Profile".to_string()),
        ("2".to_string(), "Billing".to_string()),
        ("3".to_string(), "Notifications".to_string()),
    ]);
    let current_page = Signal::new("Overview".to_string());

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = {{
        let rows = rows.clone();
        let current_page = current_page.clone();
        std::rc::Rc::new(move |msg| match msg {{
            Msg::Nav(page) => current_page.set(page),
            Msg::Edit(id) => current_page.set(format!("Edit {{id}}")),
            Msg::Delete(id) => {{
                let v: Vec<(String, String)> =
                    rows.get().into_iter().filter(|(r, _)| r != &id).collect();
                rows.set(v);
            }}
        }})
    }};

    let root = document.create_element("div")?;
    body.append_child(&root)?;

    let rows_for_ui = rows.clone();
    let current_page_for_ui = current_page.clone();
    let ui: UITree<Msg> = dashboard_shell(&DashboardShellConfig {{
        title: "{app_title}".to_string(),
        nav_items: vec!["Overview".into(), "Settings".into(), "Help".into()],
        content: Box::new(move |c| {{
            let rows = rows_for_ui.clone();
            let current_page = current_page_for_ui.clone();
            let inner = settings_list(&SettingsListConfig {{
                title: current_page.get(),
                rows: rows.get(),
                on_edit: Box::new(Msg::Edit),
                on_delete: Box::new(Msg::Delete),
            }});
            c.with(inner);
        }}),
        on_nav: Box::new(Msg::Nav),
    }});

    let handle = tpt_appfront_dom::mount(&root, &ui, dispatch)?;
    std::mem::forget(handle);
    Ok(())
}}
"#
    )
}

fn crud_app_preset_lib_rs(app_title: &str) -> String {
    format!(
        r#"//! CRUD app preset — scaffolded by `tpt-appfront init --preset crud-app`.
//! A focused `settings_list` (from `tpt_appfront_templates`) plus an "Add row"
//! button, all wired to a `Signal`-backed list. Edit/Delete/Add dispatch `Msg`s
//! that mutate the shared list.

use tpt_appfront_core::{{Signal, UITree}};
use tpt_appfront_templates::{{settings_list, SettingsListConfig}};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {{
    Add,
    Edit(String),
    Delete(String),
}}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {{
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let rows = Signal::new(vec![
        ("1".to_string(), "First item".to_string()),
        ("2".to_string(), "Second item".to_string()),
    ]);

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = {{
        let rows = rows.clone();
        std::rc::Rc::new(move |msg| match msg {{
            Msg::Add => {{
                let next: i32 = (rows.get().len() as i32) + 1;
                let mut v = rows.get();
                v.push((next.to_string(), format!("New item {{next}}")));
                rows.set(v);
            }}
            Msg::Edit(id) => {{
                let mut v = rows.get();
                if let Some(row) = v.iter_mut().find(|(r, _)| r == &id) {{
                    row.1 = format!("{{}} (edited)", row.1);
                }}
                rows.set(v);
            }}
            Msg::Delete(id) => {{
                let v: Vec<(String, String)> =
                    rows.get().into_iter().filter(|(r, _)| r != &id).collect();
                rows.set(v);
            }}
        }})
    }};

    let root = document.create_element("div")?;
    body.append_child(&root)?;

    let rows_for_ui = rows.clone();
    let ui: UITree<Msg> = UITree::container(|c| {{
        c.button("Add row").on_click(Msg::Add);
        let inner = settings_list(&SettingsListConfig {{
            title: "{app_title}".to_string(),
            rows: rows_for_ui.get(),
            on_edit: Box::new(Msg::Edit),
            on_delete: Box::new(Msg::Delete),
        }});
        c.with(inner);
    }});

    let handle = tpt_appfront_dom::mount(&root, &ui, dispatch)?;
    std::mem::forget(handle);
    Ok(())
}}
"#
    )
}

fn saas_preset_lib_rs(app_title: &str) -> String {
    format!(
        r#"//! SaaS starter preset — scaffolded by `tpt-appfront init --preset saas-starter`.
//! A full app shell (`dashboard_shell`) whose content switches per route, with
//! a `settings_list` mounted under the Settings route. Mirrors a typical SaaS
//! dashboard layout, all from `tpt_appfront_templates` + `Signal` state.

use tpt_appfront_core::{{Signal, UITree}};
use tpt_appfront_templates::{{
    dashboard_shell, settings_list, DashboardShellConfig, SettingsListConfig,
}};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {{
    Nav(String),
    Edit(String),
    Delete(String),
}}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {{
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let rows = Signal::new(vec![
        ("1".to_string(), "Profile".to_string()),
        ("2".to_string(), "Billing".to_string()),
        ("3".to_string(), "Team".to_string()),
    ]);
    let current_page = Signal::new("Dashboard".to_string());

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = {{
        let rows = rows.clone();
        let current_page = current_page.clone();
        std::rc::Rc::new(move |msg| match msg {{
            Msg::Nav(page) => current_page.set(page),
            Msg::Edit(id) => current_page.set(format!("Edit {{id}}")),
            Msg::Delete(id) => {{
                let v: Vec<(String, String)> =
                    rows.get().into_iter().filter(|(r, _)| r != &id).collect();
                rows.set(v);
            }}
        }})
    }};

    let root = document.create_element("div")?;
    body.append_child(&root)?;

    let rows_for_ui = rows.clone();
    let current_page_for_ui = current_page.clone();
    let ui: UITree<Msg> = dashboard_shell(&DashboardShellConfig {{
        title: "{app_title}".to_string(),
        nav_items: vec![
            "Dashboard".into(),
            "Customers".into(),
            "Billing".into(),
            "Settings".into(),
        ],
        content: Box::new(move |c| {{
            let rows = rows_for_ui.clone();
            let current_page = current_page_for_ui.clone();
            match current_page.get().as_str() {{
                "Settings" => {{
                    let inner = settings_list(&SettingsListConfig {{
                        title: "Settings".to_string(),
                        rows: rows.get(),
                        on_edit: Box::new(Msg::Edit),
                        on_delete: Box::new(Msg::Delete),
                    }});
                    c.with(inner);
                }}
                page => {{
                    c.heading(2, page.to_string());
                    c.text(format!("Content for the {{page}} route goes here."));
                }}
            }}
        }}),
        on_nav: Box::new(Msg::Nav),
    }});

    let handle = tpt_appfront_dom::mount(&root, &ui, dispatch)?;
    std::mem::forget(handle);
    Ok(())
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looks_like_toml(s: &str) -> bool {
        s.contains("[package]") && s.contains("[dependencies]")
    }

    #[test]
    fn canvas_cargo_toml_embeds_paths_and_is_toml_shaped() {
        let out = canvas_cargo_toml(
            "my-app",
            "path = \"/repo/tpt-appfront-core\"",
            "path = \"/repo/tpt-appfront-canvas\"",
        );
        assert!(out.contains("tpt-appfront-core = path = \"/repo/tpt-appfront-core\""));
        assert!(out.contains("tpt-appfront-canvas = path = \"/repo/tpt-appfront-canvas\""));
        assert!(out.contains("name = \"my-app\""));
        assert!(looks_like_toml(&out));
    }

    #[test]
    fn dom_cargo_toml_embeds_paths_and_is_toml_shaped() {
        let out = dom_cargo_toml(
            "my-app",
            "path = \"/repo/tpt-appfront-core\"",
            "path = \"/repo/tpt-appfront-dom\"",
        );
        assert!(out.contains("tpt-appfront-core = path = \"/repo/tpt-appfront-core\""));
        assert!(out.contains("tpt-appfront-dom = path = \"/repo/tpt-appfront-dom\""));
        assert!(out.contains("crate-type = [\"cdylib\", \"rlib\"]"));
        assert!(looks_like_toml(&out));
    }

    #[test]
    fn tui_cargo_toml_embeds_paths_and_is_toml_shaped() {
        let out = tui_cargo_toml(
            "my-app",
            "path = \"/repo/tpt-appfront-core\"",
            "path = \"/repo/tpt-appfront-tui\"",
        );
        assert!(out.contains("tpt-appfront-core = path = \"/repo/tpt-appfront-core\""));
        assert!(out.contains("tpt-appfront-tui = path = \"/repo/tpt-appfront-tui\""));
        assert!(looks_like_toml(&out));
    }

    #[test]
    fn canvas_main_rs_interpolates_title_with_no_leftover_braces() {
        let out = canvas_main_rs("My App");
        assert!(out.contains("My App"));
        assert!(!out.contains("{{"));
        assert!(!out.contains("}}"));
    }

    #[test]
    fn dom_lib_rs_interpolates_title_with_no_leftover_braces() {
        let out = dom_lib_rs("My App");
        assert!(out.contains("My App"));
        assert!(!out.contains("{{"));
        assert!(!out.contains("}}"));
    }

    #[test]
    fn tui_main_rs_interpolates_title_with_no_leftover_braces() {
        let out = tui_main_rs("My App");
        assert!(out.contains("My App"));
        assert!(!out.contains("{{"));
        assert!(!out.contains("}}"));
    }

    #[test]
    fn server_cargo_toml_embeds_paths_and_tokio() {
        let out = server_cargo_toml(
            "my-server",
            "path = \"/repo/tpt-appfront-core\"",
            "path = \"/repo/tpt-appfront-server\"",
        );
        assert!(out.contains("tpt-appfront-core = path = \"/repo/tpt-appfront-core\""));
        assert!(out.contains(
            "tpt-appfront-server = path = \"/repo/tpt-appfront-server\""
        ));
        assert!(out.contains("tokio = { version = \"1\", features = [\"full\"] }"));
        assert!(looks_like_toml(&out));
    }

    #[test]
    fn server_main_rs_wires_smart_router_and_has_no_leftover_braces() {
        let out = server_main_rs("My App");
        assert!(out.contains("SmartRouterBuilder::new"));
        assert!(out.contains("tpt_appfront_server::{SmartRouterBuilder, serve}"));
        assert!(out.contains("serve(router, addr).await;"));
        assert!(!out.contains("{{"));
    }

    #[test]
    fn index_html_interpolates_title() {
        let out = index_html("My App");
        assert!(out.contains("<title>My App</title>"));
    }

    #[test]
    fn gitignore_has_expected_entries() {
        assert_eq!(gitignore(), "/target\n/dist\nCargo.lock\n");
    }

    #[test]
    fn packager_toml_mentions_formats_and_updates() {
        let out = packager_toml("my-app");
        assert!(out.contains("product-name = \"my-app\""));
        assert!(out.contains("formats = ["));
        assert!(out.contains("generate-updates = true"));
    }

    #[test]
    fn readme_both_mentions_canvas_and_dom() {
        let out = readme("my-app", true);
        assert!(out.contains("# my-app"));
        assert!(out.contains("canvas/"));
        assert!(out.contains("dom/"));
    }

    #[test]
    fn readme_single_target_is_minimal() {
        let out = readme("my-app", false);
        assert!(out.contains("# my-app"));
        assert!(!out.contains("canvas/"));
    }

    #[test]
    fn component_rs_is_view_macro_shaped() {
        let out = component_rs("UserBadge", "user-badge");
        assert!(out.contains("pub fn UserBadge() -> UITree<Msg>"));
        assert!(out.contains("class=\"user-badge\""));
        assert!(out.contains("view!"));
        assert!(!out.contains("{{"));
        assert!(!out.contains("}}"));
    }

    #[test]
    fn page_rs_is_view_macro_shaped() {
        let out = page_rs("Settings", "settings");
        assert!(out.contains("pub fn Settings() -> UITree<Msg>"));
        assert!(out.contains("class=\"settings\""));
        assert!(out.contains("view!"));
    }

    #[test]
    fn preset_cargo_toml_includes_templates_dep_and_is_toml_shaped() {
        let out = preset_cargo_toml(
            "my-app",
            "path = \"/repo/tpt-appfront-core\"",
            "path = \"/repo/tpt-appfront-dom\"",
            "path = \"/repo/tpt-appfront-templates\"",
        );
        assert!(out.contains("tpt-appfront-core = path = \"/repo/tpt-appfront-core\""));
        assert!(out.contains("tpt-appfront-dom = path = \"/repo/tpt-appfront-dom\""));
        assert!(out.contains("tpt-appfront-templates = path = \"/repo/tpt-appfront-templates\""));
        assert!(looks_like_toml(&out));
        assert!(out.contains("crate-type = [\"cdylib\", \"rlib\"]"));
    }

    fn preset_lib_has_no_escaped_braces(out: &str) {
        // A leftover `{{` would mean a `format!` escape wasn't collapsed — a real
        // authoring bug. `}}` legitimately appears in generated Rust (two adjacent
        // closing braces), so only `{{` is checked.
        assert!(!out.contains("{{"), "leftover escaped brace in:\n{out}");
    }

    #[test]
    fn login_preset_lib_uses_view_macro_and_signals() {
        let out = preset_lib_rs(&crate::presets::Preset::Login, "My App");
        assert!(out.contains("My App"));
        assert!(out.contains("use tpt_appfront_core::{Signal, UITree, view}"));
        assert!(out.contains("on_input={Msg::SetUsername}"));
        preset_lib_has_no_escaped_braces(&out);
    }

    #[test]
    fn dashboard_preset_lib_composes_templates() {
        let out = preset_lib_rs(&crate::presets::Preset::Dashboard, "My App");
        assert!(out.contains("dashboard_shell"));
        assert!(out.contains("settings_list"));
        assert!(out.contains("My App"));
        preset_lib_has_no_escaped_braces(&out);
    }

    #[test]
    fn crud_app_preset_lib_builds_list() {
        let out = preset_lib_rs(&crate::presets::Preset::CrudApp, "My App");
        assert!(out.contains("settings_list"));
        assert!(out.contains("Msg::Add"));
        assert!(out.contains("My App"));
        preset_lib_has_no_escaped_braces(&out);
    }

    #[test]
    fn saas_preset_lib_switches_content_per_route() {
        let out = preset_lib_rs(&crate::presets::Preset::SaasStarter, "My App");
        assert!(out.contains("dashboard_shell"));
        assert!(out.contains("\"Settings\""));
        assert!(out.contains("My App"));
        preset_lib_has_no_escaped_braces(&out);
    }
}
