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

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Go,
}

/// The text a reader would see under `node`. `text_content` joins adjacent nodes with no
/// separator, so `<h1>Dashboard Overview</h1>Count` would read as one word, `overviewcount`.
/// Walking the tree and putting a space after every node keeps the boundaries apart.
fn visible_text(node: &web_sys::Node, out: &mut String) {
    if node.node_type() == web_sys::Node::TEXT_NODE {
        out.push_str(&node.node_value().unwrap_or_default());
    } else {
        let children = node.child_nodes();
        for i in 0..children.length() {
            if let Some(child) = children.item(i) {
                visible_text(&child, out);
            }
        }
    }
    out.push(' ');
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

    let mut text = String::new();
    visible_text(&host, &mut text);
    let shown = words(&text);
    let missing: Vec<String> = required_words(&ui).difference(&shown).cloned().collect();
    assert!(missing.is_empty(), "the DOM dropped words: {missing:?}");
}
