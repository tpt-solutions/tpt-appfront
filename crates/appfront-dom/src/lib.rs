//! Fine-grained-reactive real DOM backend (see spec.txt's "Better DOM" pivot).
//!
//! `mount` walks a `UITree` once and creates real DOM nodes directly via
//! `web-sys` — no virtual DOM, no diffing. Event handlers dispatch an
//! app-defined `Msg` back through a caller-supplied callback. This crate
//! only does anything on `wasm32` targets; on other targets it compiles to
//! an empty crate so the workspace still builds natively.

#![cfg(target_arch = "wasm32")]

use appfront_core::{NodeKind, UITree};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, Node};

/// Renders `ui` into real DOM nodes and appends them to `container`.
/// `dispatch` is called with a cloned `Msg` whenever a bound event fires
/// (e.g. `on_click`).
pub fn mount<Msg>(
    container: &Element,
    ui: &UITree<Msg>,
    dispatch: Rc<dyn Fn(Msg)>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    let node = render_node(&document, ui, &dispatch)?;
    container.append_child(&node)?;
    Ok(())
}

fn render_node<Msg>(
    document: &Document,
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
) -> Result<Node, wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let node: Node = match &ui.kind {
        NodeKind::Container { children } => {
            let el = document.create_element("div")?;
            for child in children {
                let child_node = render_node(document, child, dispatch)?;
                el.append_child(&child_node)?;
            }
            el.into()
        }
        NodeKind::List { items } => {
            let el = document.create_element("ul")?;
            for item in items {
                let li = document.create_element("li")?;
                let item_node = render_node(document, item, dispatch)?;
                li.append_child(&item_node)?;
                el.append_child(&li)?;
            }
            el.into()
        }
        NodeKind::Heading { level, text } => {
            let tag = format!("h{}", (*level).clamp(1, 6));
            let el = document.create_element(&tag)?;
            el.set_text_content(Some(text));
            el.into()
        }
        NodeKind::Text { text } => document.create_text_node(text).into(),
        NodeKind::Button { label } => {
            let el = document.create_element("button")?;
            el.set_text_content(Some(label));
            el.into()
        }
        NodeKind::Input { value } => {
            let el = document.create_element("input")?;
            el.set_attribute("value", value)?;
            el.into()
        }
        NodeKind::DataGrid { columns, rows } => {
            let table = document.create_element("table")?;

            let thead = document.create_element("thead")?;
            let header_row = document.create_element("tr")?;
            for column in columns {
                let th = document.create_element("th")?;
                th.set_text_content(Some(column));
                header_row.append_child(&th)?;
            }
            thead.append_child(&header_row)?;
            table.append_child(&thead)?;

            let tbody = document.create_element("tbody")?;
            for row in rows {
                let tr = document.create_element("tr")?;
                for cell in row {
                    let td = document.create_element("td")?;
                    td.set_text_content(Some(cell));
                    tr.append_child(&td)?;
                }
                tbody.append_child(&tr)?;
            }
            table.append_child(&tbody)?;
            table.into()
        }
    };

    if let Some(class) = &ui.meta.class {
        if let Some(el) = node.dyn_ref::<Element>() {
            el.set_attribute("class", class)?;
        }
    }

    if let Some(msg) = ui.meta.on_click.clone() {
        let dispatch = Rc::clone(dispatch);
        let closure = Closure::<dyn FnMut()>::new(move || dispatch(msg.clone()));
        if let Some(el) = node.dyn_ref::<web_sys::HtmlElement>() {
            el.set_onclick(Some(closure.as_ref().unchecked_ref()));
        }
        // Leak the closure so it outlives this function call; the DOM node
        // holds the only remaining reference to it via `set_onclick`.
        closure.forget();
    }

    Ok(node)
}

/// Ties a `Text` DOM node's content directly to a `Signal<String>`: when the
/// signal updates, only this text node's `data` is mutated — no re-render,
/// no diffing. This is the "fine-grained reactivity" primitive from the
/// spec's counter example, usable for any dynamic leaf text.
pub fn reactive_text(
    document: &Document,
    signal: appfront_core::Signal<String>,
) -> Result<Node, wasm_bindgen::JsValue> {
    let text_node = document.create_text_node(&signal.get());
    let node_for_effect = text_node.clone();
    let handle = appfront_core::create_effect(move || {
        node_for_effect.set_data(&signal.get());
    });
    // Leak the effect handle so the subscription outlives this call; the
    // text node is the thing that should keep it alive in a full
    // component-lifecycle system (tracked as future work).
    std::mem::forget(handle);
    Ok(text_node.into())
}
