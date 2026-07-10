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

pub fn gitignore() -> &'static str {
    "/target\n/dist\nCargo.lock\n"
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
