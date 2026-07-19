//! Browser (wasm-bindgen-test) tests for the DOM backend. These run in an
//! actual browser via `wasm-pack test --chrome` (or `trunk`'s test runner),
//! not the headless native test harness — see the Phase 15 follow-up for real
//! end-to-end browser verification.

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, NodeKind, UITree};
use tpt_appfront_dom::{mount, render_node};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Document, HtmlElement};

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

/// Builds a tree exercising every form-control node kind so we can assert the
/// DOM element for each is produced and wired to the right event.
fn form_ui() -> UITree<FormMsg> {
    UITree::container(|c: &mut ContainerBuilder<FormMsg>| {
        c.input("hello")
            .on_input(FormMsg::InputChanged);
        c.textarea("notes").on_input(FormMsg::TextareaChanged);
        c.checkbox("Agree", false).on_toggle(FormMsg::Toggled);
        c.select([("a", "Alpha"), ("b", "Beta")], "a")
            .on_input(FormMsg::SelectChanged);
        c.radio_group("color", [("r", "Red"), ("g", "Green")], "r")
            .on_input(FormMsg::RadioChanged);
    })
}

#[derive(Clone, PartialEq, Debug)]
enum FormMsg {
    InputChanged(String),
    TextareaChanged(String),
    Toggled(bool),
    SelectChanged(String),
    RadioChanged(String),
}

#[wasm_bindgen_test]
fn renders_all_form_controls() {
    let doc = document();
    let ui = form_ui();
    let node = render_node(&doc, &ui, &Rc::new(|_| {})).unwrap();

    let container = node.dyn_ref::<web_sys::Element>().unwrap();
    assert_eq!(container.tag_name().to_ascii_lowercase(), "div");

    // Input
    assert!(container.query_selector("input[type=text]").ok().flatten().is_some()
        || container.query_selector("input:not([type])").ok().flatten().is_some());
    // Textarea
    let ta = container.query_selector("textarea").ok().flatten().unwrap();
    assert_eq!(ta.text_content().as_deref(), Some("notes"));
    // Checkbox label wraps an input[type=checkbox]
    let cb = container
        .query_selector("label input[type=checkbox]")
        .ok()
        .flatten()
        .unwrap();
    assert!(!cb.dyn_ref::<web_sys::HtmlInputElement>().unwrap().checked());
    // Select with option
    let sel = container.query_selector("select").ok().flatten().unwrap();
    assert_eq!(sel.children().length(), 2);
    assert!(sel
        .query_selector("option[selected]")
        .ok()
        .flatten()
        .is_some());
    // Radio group
    let radios = container.query_selector_all("input[type=radio]").ok().unwrap();
    assert_eq!(radios.length(), 2);
    let checked_radio = container
        .query_selector("input[type=radio][checked]")
        .ok()
        .flatten()
        .unwrap();
    assert_eq!(
        checked_radio
            .dyn_ref::<web_sys::HtmlInputElement>()
            .unwrap()
            .value(),
        "r"
    );
}

#[wasm_bindgen_test]
fn on_input_fires_for_textarea_and_select() {
    let doc = document();
    let host = doc.create_element("div").unwrap();
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .body()
        .unwrap()
        .append_child(&host)
        .unwrap();

    let received = Rc::new(std::cell::RefCell::new(Vec::new()));
    let received_clone = received.clone();
    let dispatch: Rc<dyn Fn(FormMsg)> = Rc::new(move |m| received_clone.borrow_mut().push(m));

    let ui = UITree::container(|c: &mut ContainerBuilder<FormMsg>| {
        c.textarea("notes").on_input(FormMsg::TextareaChanged);
        c.select([("a", "Alpha"), ("b", "Beta")], "a")
            .on_input(FormMsg::SelectChanged);
    });

    let _root = mount(&host, &ui, dispatch).unwrap();

    let ta = host
        .query_selector("textarea")
        .ok()
        .flatten()
        .unwrap()
        .dyn_into::<web_sys::HtmlTextAreaElement>()
        .unwrap();
    ta.set_value("typed");
    ta.dispatch_event(&web_sys::Event::new("input").unwrap())
        .unwrap();

    let sel = host
        .query_selector("select")
        .ok()
        .flatten()
        .unwrap()
        .dyn_into::<web_sys::HtmlSelectElement>()
        .unwrap();
    sel.set_value("b");
    sel.dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();

    let got = received.borrow().clone();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], FormMsg::TextareaChanged("typed".to_string()));
    assert_eq!(got[1], FormMsg::SelectChanged("b".to_string()));
}

#[wasm_bindgen_test]
fn on_toggle_fires_for_checkbox() {
    let doc = document();
    let host = doc.create_element("div").unwrap();
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .body()
        .unwrap()
        .append_child(&host)
        .unwrap();

    let received = Rc::new(std::cell::RefCell::new(None));
    let received_clone = received.clone();
    let dispatch: Rc<dyn Fn(FormMsg)> = Rc::new(move |m| *received_clone.borrow_mut() = Some(m));

    let ui = UITree::container(|c: &mut ContainerBuilder<FormMsg>| {
        c.checkbox("Agree", false).on_toggle(FormMsg::Toggled);
    });

    let _root = mount(&host, &ui, dispatch).unwrap();

    let cb = host
        .query_selector("label input[type=checkbox]")
        .ok()
        .flatten()
        .unwrap()
        .dyn_into::<web_sys::HtmlInputElement>()
        .unwrap();
    cb.set_checked(true);
    cb.dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();

    assert_eq!(*received.borrow(), Some(FormMsg::Toggled(true)));
}

// Keeps `mount`/`NodeKind` imports referenced on native-style builds.
#[allow(unused)]
fn _assert_imports() {
    let _ = HtmlElement::class_list;
    let _ = NodeKind::Text { text: String::new() };
}
