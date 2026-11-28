//! DOM text parity: every visible word the core tree declares is shown in the DOM.
//!
//! This is a browser test (see `browser.rs` for how the wasm runner is used). It
//! uses the core word helpers, not `tpt_appfront_test::parity`, because that module
//! pulls in the TUI backend, which does not build for wasm32.

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use tpt_appfront_core::text_words::{required_words, words};
use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_dom::mount;
use wasm_bindgen_test::*;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Go,
}

#[wasm_bindgen_test]
fn dom_shows_every_declared_word() {
    let ui: UITree<Msg> = UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Dashboard Overview");
        c.text("Count is zero");
        c.button("Submit").on_click(Msg::Go);
    });

    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let _root = mount(&host, &ui, Rc::new(|_: Msg| {})).unwrap();

    let shown = words(&host.text_content().unwrap_or_default());
    let missing: Vec<String> = required_words(&ui).difference(&shown).cloned().collect();
    assert!(missing.is_empty(), "the DOM dropped words: {missing:?}");
}
