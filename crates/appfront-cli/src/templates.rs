//! String templates used by `appfront init` to scaffold a new project.
//! Path dependencies point at the `tpt-appfront` checkout that built this
//! CLI binary (via `CARGO_MANIFEST_DIR`), so the generated project builds
//! with zero manual edits as long as it's run against that same checkout.
//! Once the crates are published this switches to version dependencies.

pub fn canvas_cargo_toml(pkg_name: &str, core_path: &str, canvas_path: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
appfront-core = {{ path = "{core_path}" }}
appfront-canvas = {{ path = "{canvas_path}" }}
"#
    )
}

pub fn canvas_main_rs(app_title: &str) -> String {
    format!(
        r#"use appfront_core::{{Signal, UITree}};

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

    appfront_canvas::run_native("{app_title}", build_ui, dispatch)?;
    Ok(())
}}
"#
    )
}

pub fn dom_cargo_toml(pkg_name: &str, core_path: &str, dom_path: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
appfront-core = {{ path = "{core_path}" }}
appfront-dom = {{ path = "{dom_path}" }}
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
        r#"use appfront_core::{{create_effect, Signal, UITree}};
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

    appfront_dom::mount(&container, &ui, dispatch)?;

    let (text_node, text_handle) = appfront_dom::reactive_text(&document, display)?;
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

pub fn tui_cargo_toml(pkg_name: &str, core_path: &str, tui_path: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
appfront-core = {{ path = "{core_path}" }}
appfront-tui = {{ path = "{tui_path}" }}
"#
    )
}

pub fn tui_main_rs(app_title: &str) -> String {
    format!(
        r#"use appfront_core::{{Signal, UITree}};

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

    // Tab/Arrows move focus, Enter/Space activate, Esc quits.
    appfront_tui::run(build_ui, dispatch)?;
    Ok(())
}}
"#
    )
}

pub fn gitignore() -> &'static str {
    "/target\n/dist\nCargo.lock\n"
}

/// `cargo-packager` config (`packager.toml`) written by `appfront build
/// --bundle` / `appfront optimize --bundle` when one isn't already present.
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

Scaffolded by `appfront init`.

- `canvas/` — native desktop app (winit/egui via `appfront-canvas`). Run with:
  ```sh
  cd canvas && cargo run
  ```
- `dom/` — browser app (real DOM via `appfront-dom`). Run with:
  ```sh
  cd dom && trunk serve
  ```

Or drive both through the CLI from this directory:
```sh
appfront dev --desktop --project canvas
appfront dev --web --project dom
appfront build --target canvas --project canvas
appfront build --target dom --project dom
```
"#
        )
    } else {
        format!(
            r#"# {name}

Scaffolded by `appfront init`. See `appfront dev --help` / `appfront build --help`.
"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looks_like_toml(s: &str) -> bool {
        s.contains("[package]") && s.contains("[dependencies]")
    }

    #[test]
    fn canvas_cargo_toml_embeds_paths_and_is_toml_shaped() {
        let out = canvas_cargo_toml("my-app", "/repo/appfront-core", "/repo/appfront-canvas");
        assert!(out.contains("/repo/appfront-core"));
        assert!(out.contains("/repo/appfront-canvas"));
        assert!(out.contains("name = \"my-app\""));
        assert!(looks_like_toml(&out));
    }

    #[test]
    fn dom_cargo_toml_embeds_paths_and_is_toml_shaped() {
        let out = dom_cargo_toml("my-app", "/repo/appfront-core", "/repo/appfront-dom");
        assert!(out.contains("/repo/appfront-core"));
        assert!(out.contains("/repo/appfront-dom"));
        assert!(out.contains("crate-type = [\"cdylib\", \"rlib\"]"));
        assert!(looks_like_toml(&out));
    }

    #[test]
    fn tui_cargo_toml_embeds_paths_and_is_toml_shaped() {
        let out = tui_cargo_toml("my-app", "/repo/appfront-core", "/repo/appfront-tui");
        assert!(out.contains("/repo/appfront-core"));
        assert!(out.contains("/repo/appfront-tui"));
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
}
