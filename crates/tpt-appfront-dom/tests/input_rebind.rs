//! A re-rendered text input must deliver its current `on_input` message, not the one it had at mount.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_dom::mount;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, Event, HtmlInputElement};

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Typed(String, u32),
}

fn search(tag: u32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.input("")
            .key("query")
            .attr("aria-label", "Search")
            .on_input(move |value| Msg::Typed(value, tag));
    })
}

#[wasm_bindgen_test]
fn rerendered_input_delivers_the_current_message() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let sink = Rc::new(RefCell::new(Vec::new()));
    let recorder = sink.clone();
    let mut root = mount(
        &container,
        &search(0),
        Rc::new(move |msg| recorder.borrow_mut().push(msg)),
    )
    .unwrap();

    root.render(&search(5))
        .expect("same handler set renders in place");

    let input: HtmlInputElement = container
        .query_selector("input")
        .unwrap()
        .expect("the input is rendered")
        .dyn_into()
        .unwrap();
    input.set_value("ada");
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();

    assert_eq!(
        sink.borrow().as_slice(),
        &[Msg::Typed("ada".to_string(), 5)],
        "the input event must carry the re-rendered message"
    );
    root.unmount();
}
