//! Browser (wasm-bindgen-test) tests for the focus, keyboard, pointer and paste handlers. CI
//! runs them in headless Chromium, as it does `form_controls.rs`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use tpt_appfront_core::{KeyEvent, UITree};
use tpt_appfront_dom::mount;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Event, KeyboardEvent, KeyboardEventInit, PointerEvent, PointerEventInit};

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum EventMsg {
    Focus,
    Blur,
    Key(String),
    Down(i16, f64),
}

/// Mounts a single button with every handler set, and returns it with the log of dispatched
/// messages.
fn mounted_button() -> (web_sys::HtmlElement, Rc<RefCell<Vec<EventMsg>>>) {
    let ui = UITree::container(|c| {
        c.button("go")
            .on_focus(EventMsg::Focus)
            .on_blur(EventMsg::Blur)
            .on_keydown(|k: KeyEvent| (k.key == "Enter").then_some(EventMsg::Key(k.key)))
            .on_pointer_down(|p| Some(EventMsg::Down(p.button, p.x)));
    });
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&log);
    let doc = web_sys::window().unwrap().document().unwrap();
    let host = doc.create_element("div").unwrap();
    doc.body().unwrap().append_child(&host).unwrap();
    mount(&host, &ui, Rc::new(move |m| sink.borrow_mut().push(m))).unwrap();
    let button = host
        .query_selector("button")
        .unwrap()
        .expect("a button")
        .dyn_into::<web_sys::HtmlElement>()
        .unwrap();
    (button, log)
}

#[wasm_bindgen_test]
fn focus_and_blur_dispatch_their_messages() {
    let (button, log) = mounted_button();
    button
        .dispatch_event(&Event::new("focus").unwrap())
        .unwrap();
    button.dispatch_event(&Event::new("blur").unwrap()).unwrap();
    assert_eq!(*log.borrow(), vec![EventMsg::Focus, EventMsg::Blur]);
}

#[wasm_bindgen_test]
fn enter_dispatches_and_is_prevented_while_other_keys_are_left_alone() {
    let (button, log) = mounted_button();

    let init = KeyboardEventInit::new();
    init.set_key("Enter");
    init.set_cancelable(true);
    let enter = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    button.dispatch_event(&enter).unwrap();
    assert_eq!(*log.borrow(), vec![EventMsg::Key("Enter".into())]);
    assert!(
        enter.default_prevented(),
        "a handled key stops the browser default"
    );

    let init = KeyboardEventInit::new();
    init.set_key("a");
    init.set_cancelable(true);
    let other = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    button.dispatch_event(&other).unwrap();
    assert_eq!(log.borrow().len(), 1, "unhandled keys dispatch nothing");
    assert!(!other.default_prevented());
}

#[wasm_bindgen_test]
fn pointer_down_passes_the_button_and_position() {
    let (button, log) = mounted_button();
    let init = PointerEventInit::new();
    init.set_client_x(12);
    init.set_button(0);
    let press = PointerEvent::new_with_event_init_dict("pointerdown", &init).unwrap();
    button.dispatch_event(&press).unwrap();
    assert_eq!(*log.borrow(), vec![EventMsg::Down(0, 12.0)]);
}
