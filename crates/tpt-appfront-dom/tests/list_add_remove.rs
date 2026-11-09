//! Adding and removing keyed list items must reconcile in place. Items that stay keep their DOM nodes,
//! and the order matches the tree.

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_dom::mount;
use wasm_bindgen_test::*;
use web_sys::{Element, Node};

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(String),
}

/// A list whose items keep their keys (`key`) when others are added or removed.
fn named(items: &[(&str, &str)]) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "People");
        c.list(|list| {
            for (key, label) in items {
                list.button(*label)
                    .key(*key)
                    .on_click(Msg::Select(key.to_string()));
            }
        });
    })
}

fn dispatch() -> Rc<dyn Fn(Msg)> {
    Rc::new(|_| {})
}

fn button(container: &Element, key: &str) -> Element {
    container
        .query_selector(&format!("button[data-key=\"{key}\"]"))
        .unwrap()
        .unwrap_or_else(|| panic!("no button with key {key}"))
}

/// Button labels in document order.
fn labels_in_order(container: &Element) -> Vec<String> {
    let html = container.inner_html();
    let mut out = Vec::new();
    let mut rest = html.as_str();
    while let Some(start) = rest.find("data-key=\"") {
        rest = &rest[start..];
        let close = rest.find('>').unwrap();
        let end = rest[close..].find("</button>").unwrap() + close;
        out.push(rest[close + 1..end].to_string());
        rest = &rest[end..];
    }
    out
}

#[wasm_bindgen_test]
fn appending_an_item_keeps_the_existing_nodes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let mut root = mount(
        &container,
        &named(&[("a", "Ada"), ("b", "Grace")]),
        dispatch(),
    )
    .unwrap();
    let ada_before = button(&container, "a");

    root.render(&named(&[("a", "Ada"), ("b", "Grace"), ("c", "Linus")]))
        .expect("adding an item renders in place");

    assert_eq!(labels_in_order(&container), ["Ada", "Grace", "Linus"]);
    let ada_after: Node = button(&container, "a").into();
    assert!(
        ada_before.is_same_node(Some(&ada_after)),
        "an item that stays keeps its node"
    );
    root.unmount();
}

#[wasm_bindgen_test]
fn removing_an_item_keeps_the_others() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let mut root = mount(
        &container,
        &named(&[("a", "Ada"), ("b", "Grace"), ("c", "Linus")]),
        dispatch(),
    )
    .unwrap();
    let grace_before = button(&container, "b");

    root.render(&named(&[("b", "Grace"), ("c", "Linus")]))
        .expect("removing an item renders in place");

    assert_eq!(labels_in_order(&container), ["Grace", "Linus"]);
    let grace_after: Node = button(&container, "b").into();
    assert!(grace_before.is_same_node(Some(&grace_after)));
    assert!(
        container
            .query_selector("button[data-key=\"a\"]")
            .unwrap()
            .is_none(),
        "the removed item is gone from the DOM"
    );
    root.unmount();
}

#[wasm_bindgen_test]
fn inserting_in_the_middle_keeps_order_and_nodes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let mut root = mount(
        &container,
        &named(&[("a", "Ada"), ("c", "Linus")]),
        dispatch(),
    )
    .unwrap();
    let linus_before = button(&container, "c");

    root.render(&named(&[("a", "Ada"), ("b", "Grace"), ("c", "Linus")]))
        .expect("inserting renders in place");

    assert_eq!(labels_in_order(&container), ["Ada", "Grace", "Linus"]);
    let linus_after: Node = button(&container, "c").into();
    assert!(linus_before.is_same_node(Some(&linus_after)));
    root.unmount();
}

#[wasm_bindgen_test]
fn emptying_and_refilling_a_list_renders() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let mut root = mount(&container, &named(&[("a", "Ada")]), dispatch()).unwrap();

    root.render(&named(&[])).expect("an empty list renders");
    assert!(labels_in_order(&container).is_empty());

    root.render(&named(&[("z", "Zed")]))
        .expect("refilling renders");
    assert_eq!(labels_in_order(&container), ["Zed"]);
    root.unmount();
}
