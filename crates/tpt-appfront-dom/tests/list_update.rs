//! Regression tests for re-rendering keyed lists on the DOM backend. Before the fix, `mount` wrapped
//! each item in an `<li>` that the keyed diff did not expect, so `render` failed for any list.

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_dom::mount;
use wasm_bindgen_test::*;
use web_sys::Element;

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(u32),
}

fn people(labels: &[&str]) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "People");
        c.list(|list| {
            for (id, label) in labels.iter().enumerate() {
                list.button(*label)
                    .key(id.to_string())
                    .on_click(Msg::Select(id as u32));
            }
        });
    })
}

fn dispatch() -> Rc<dyn Fn(Msg)> {
    Rc::new(|_| {})
}

#[wasm_bindgen_test]
fn render_updates_labels_of_a_keyed_list_in_place() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let mut root = mount(&container, &people(&["Ada", "Grace", "Linus"]), dispatch()).unwrap();

    root.render(&people(&["Ada", "Hopper", "Linus"]))
        .expect("same-length list must render without a full replace");

    let html = container.inner_html();
    assert!(html.contains("Hopper"), "changed label is shown: {html}");
    assert!(!html.contains("Grace"), "old label is gone: {html}");
    assert_eq!(
        html.matches("<button").count(),
        3,
        "no items were added or lost"
    );
    assert!(
        html.contains("data-key=\"1\""),
        "keys survive the update: {html}"
    );
    root.unmount();
}

#[wasm_bindgen_test]
fn mounted_list_items_are_keyed_bare_nodes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let root = mount(&container, &people(&["Ada", "Grace"]), dispatch()).unwrap();

    // Items sit directly under the list's <ul>, each carrying its key. The old <li> wrapper
    // would put a `<li>` between them.
    let html = container.inner_html();
    assert!(!html.contains("<li"), "no per-item wrapper: {html}");
    assert!(
        html.contains("data-key=\"0\">Ada</button>"),
        "bare item with its key: {html}"
    );
    root.unmount();
}
