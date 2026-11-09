//! Windowed (virtual-scroll) lists re-render in place: the visible window is rebuilt and the closures
//! of the rows it replaces are released. Each re-render delivers the current messages.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, UITree, VirtualScroll};
use tpt_appfront_dom::mount;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement};

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Select(u32, String),
}

const ROWS: u32 = 1_000;

fn view(label: &str, changed: Option<u32>, scroll_offset: f32) -> UITree<Msg> {
    let vs = VirtualScroll {
        item_height: 20.0,
        viewport_height: 100.0,
        scroll_offset,
        overscan: 1,
    };
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Rows");
        c.list(|list| {
            for id in 0..ROWS {
                let text = if changed == Some(id) {
                    format!("Row {id} {label}!")
                } else {
                    format!("Row {id} {label}")
                };
                list.button(text)
                    .key(id.to_string())
                    .on_click(Msg::Select(id, label.to_string()));
            }
        })
        .virtual_scroll(vs);
    })
}

fn visible_keys(scroll_offset: f32) -> Vec<u32> {
    let vs = VirtualScroll {
        item_height: 20.0,
        viewport_height: 100.0,
        scroll_offset,
        overscan: 1,
    };
    let range = vs.visible_range(ROWS as usize);
    (range.start..range.end).map(|i| i as u32).collect()
}

type Sink = Rc<RefCell<Vec<Msg>>>;

fn recording() -> (Sink, Rc<dyn Fn(Msg)>) {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let recorder = sink.clone();
    (sink, Rc::new(move |msg| recorder.borrow_mut().push(msg)))
}

#[wasm_bindgen_test]
fn rerender_of_a_visible_row_shows_it_and_delivers_its_message() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let (sink, dispatch) = recording();
    let mut root = mount(&container, &view("a", None, 0.0), dispatch).unwrap();

    let visible = visible_keys(0.0);
    let target = visible[1];
    root.render(&view("a", Some(target), 0.0))
        .expect("a windowed list re-renders in place");

    let html = container.inner_html();
    assert!(
        html.contains(&format!("Row {target} a!")),
        "the changed visible row is shown"
    );

    let button: HtmlElement = container
        .query_selector(&format!("li[data-key=\"{target}\"] > button"))
        .unwrap()
        .expect("the row is in the window")
        .dyn_into()
        .unwrap();
    button.click();
    assert_eq!(
        sink.borrow().as_slice(),
        &[Msg::Select(target, "a".to_string())],
        "the click carries the re-rendered row's message"
    );
    root.unmount();
}

#[wasm_bindgen_test]
fn scrolling_moves_the_window_without_a_full_replace() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let (_sink, dispatch) = recording();
    let mut root = mount(&container, &view("a", None, 0.0), dispatch).unwrap();

    let far = 400.0;
    root.render(&view("a", None, far))
        .expect("scrolling renders in place");

    let far_keys = visible_keys(far);
    assert!(
        container
            .query_selector(&format!("li[data-key=\"{}\"]", far_keys[0]))
            .unwrap()
            .is_some(),
        "the window now shows rows from the new scroll position: expected key {} in {}",
        far_keys[0],
        container.inner_html().chars().take(400).collect::<String>()
    );
    assert!(
        container
            .query_selector("li[data-key=\"0\"]")
            .unwrap()
            .is_none(),
        "rows that scrolled out of the window are gone"
    );
    root.unmount();
}

#[wasm_bindgen_test]
fn switching_a_windowed_list_to_a_plain_list_requires_a_full_replace() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container: Element = doc.create_element("div").unwrap();
    let (_sink, dispatch) = recording();
    let mut root = mount(&container, &view("a", None, 0.0), dispatch).unwrap();

    let plain = UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Rows");
        c.list(|list| {
            list.button("Row 0 a")
                .key("0")
                .on_click(Msg::Select(0, "a".into()));
        });
    });
    assert!(
        root.render(&plain).is_err(),
        "a layout change cannot be diffed and must report a full replace"
    );
    root.unmount();
}
