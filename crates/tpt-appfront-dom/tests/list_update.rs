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

fn recording_dispatch(sink: Rc<std::cell::RefCell<Vec<Msg>>>) -> Rc<dyn Fn(Msg)> {
    Rc::new(move |msg| sink.borrow_mut().push(msg))
}

fn people_offset(labels: &[&str], offset: u32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "People");
        c.list(|list| {
            for (id, label) in labels.iter().enumerate() {
                list.button(*label)
                    .key(id.to_string())
                    .on_click(Msg::Select(id as u32 + offset));
            }
        });
    })
}

#[wasm_bindgen_test]
fn rerender_delivers_the_current_message_on_click() {
    use wasm_bindgen::JsCast;

    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let sink = Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut root = mount(
        &container,
        &people_offset(&["Ada", "Grace"], 0),
        recording_dispatch(sink.clone()),
    )
    .unwrap();

    // Same labels and keys, new messages. The old click handler must not still send the old ones.
    root.render(&people_offset(&["Ada", "Grace"], 10))
        .expect("same handler set renders in place");

    let grace: web_sys::HtmlElement = container
        .query_selector("button[data-key=\"1\"]")
        .unwrap()
        .expect("Grace is rendered")
        .dyn_into()
        .unwrap();
    grace.click();

    assert_eq!(
        sink.borrow().as_slice(),
        &[Msg::Select(11)],
        "the click must carry the re-rendered message"
    );
    root.unmount();
}

#[wasm_bindgen_test]
fn adding_or_removing_a_handler_requires_full_replace() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let mut root = mount(&container, &people(&["Ada", "Grace"]), dispatch()).unwrap();

    // Same keys, but one button loses its click handler. Clearing a handler in place is not
    // supported, so the render must say so instead of leaving a live handler behind.
    let without_handler = UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "People");
        c.list(|list| {
            list.button("Ada").key("0").on_click(Msg::Select(0));
            list.button("Grace").key("1");
        });
    });
    assert!(
        root.render(&without_handler).is_err(),
        "a removed handler must report a full replace"
    );
    root.unmount();
}
