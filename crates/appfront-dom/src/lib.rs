//! Fine-grained-reactive real DOM backend (see spec.txt's "Better DOM" pivot).
//!
//! `mount` walks a `UITree` once and creates real DOM nodes directly via
//! `web-sys` — no virtual DOM, no diffing. Event handlers dispatch an
//! app-defined `Msg` back through a caller-supplied callback. This crate
//! only does anything on `wasm32` targets; on other targets it compiles to
//! an empty crate so the workspace still builds natively.
//!
//! ## Hydration
//!
//! [`hydrate`] is the counterpart to server-side rendering (SSR). Instead of
//! creating fresh DOM nodes, it reads the serialised `HydrationPayload` from
//! `<script id="__APPFRONT_STATE__">`, matches each `UITree` node to its
//! server-rendered DOM element via `data-appfront-id`, and attaches event
//! listeners — no DOM mutation.

#![cfg(target_arch = "wasm32")]

use appfront_core::{HydrationPayload, NodeKind, UITree};
use std::collections::HashMap;
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
            for (i, item) in items.iter().enumerate() {
                let li = document.create_element("li")?;
                li.set_attribute("data-key", &item_key(item, i))?;
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
            el.set_attribute("type", "button")?;
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

    if let Some(action) = &ui.meta.ai.action {
        if let Some(el) = node.dyn_ref::<Element>() {
            el.set_attribute("data-ai-action", action)?;
            if !ui.meta.ai.params.is_empty() {
                let params_json = serde_json::to_string(&json_obj(&ui.meta.ai.params))
                    .unwrap_or_default();
                el.set_attribute("data-ai-params", &params_json)?;
            }
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

/// Identity used to match a `List` item across renders: the explicit
/// [`appfront_core::NodeMeta::key`] if set, otherwise the item's position.
/// Falling back to position means unkeyed lists still diff correctly for
/// pure appends/truncations, but a reorder of unkeyed items is indistinguishable
/// from in-place content changes — callers that reorder should set `.key(..)`.
fn item_key<Msg>(item: &UITree<Msg>, index: usize) -> String {
    item.meta.key.clone().unwrap_or_else(|| index.to_string())
}

/// Reconciles a rendered `<ul>` against a new `List`'s items, reusing
/// existing `<li>` DOM nodes for keys present in both `old_items` and
/// `new_items` instead of rebuilding the whole list. New keys get fresh
/// `<li>` elements, removed keys are deleted, and surviving elements are
/// moved into their new position via `insertBefore` (a no-op if already
/// there). `ul` must be the `<ul>` element previously produced by
/// [`render_node`]'s `List` branch (i.e. via [`mount`]) for `old_items`.
pub fn update_list<Msg>(
    document: &Document,
    ul: &Element,
    old_items: &[UITree<Msg>],
    new_items: &[UITree<Msg>],
    dispatch: &Rc<dyn Fn(Msg)>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let children = ul.children();
    let mut old_key_to_li: HashMap<String, Element> = HashMap::new();
    for (i, old_item) in old_items.iter().enumerate() {
        if let Some(li) = children.item(i as u32) {
            old_key_to_li.insert(item_key(old_item, i), li);
        }
    }

    let mut used_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, new_item) in new_items.iter().enumerate() {
        let key = item_key(new_item, i);
        let li = if let Some(existing) = old_key_to_li.get(&key) {
            used_keys.insert(key.clone());
            existing.set_inner_html("");
            let item_node = render_node(document, new_item, dispatch)?;
            existing.append_child(&item_node)?;
            existing.clone()
        } else {
            let li = document.create_element("li")?;
            li.set_attribute("data-key", &key)?;
            let item_node = render_node(document, new_item, dispatch)?;
            li.append_child(&item_node)?;
            li
        };

        // Snapshot of whatever currently occupies position `i` in the live
        // DOM, taken *before* touching `li`. `insertBefore` is a no-op when
        // `li` already is that node, and otherwise moves/inserts it there,
        // shifting the reference node (and everything after it) down by one.
        let reference: Option<Element> = ul.children().item(i as u32);
        let already_in_place = match reference.as_deref() {
            Some(r) => li.is_same_node(Some(r)),
            None => false,
        };
        if !already_in_place {
            ul.insert_before(&li, reference.as_deref())?;
        }
    }

    for (old_key, li) in old_key_to_li.iter() {
        if !used_keys.contains(old_key) {
            ul.remove_child(li)?;
        }
    }

    Ok(())
}

/// Converts `Vec<(String, String)>` to a JSON object for `data-ai-params`.
fn json_obj(pairs: &[(String, String)]) -> serde_json::Value {
    use std::collections::BTreeMap;
    let mut map = BTreeMap::new();
    for (k, v) in pairs {
        map.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    serde_json::Value::Object(map.into_iter().collect())
}

/// Ties a `Text` DOM node's content directly to a `Signal<String>`: when the
/// signal updates, only this text node's `data` is mutated — no re-render,
/// no diffing. This is the "fine-grained reactivity" primitive from the
/// spec's counter example, usable for any dynamic leaf text.
///
/// Returns the text node along with the [`appfront_core::EffectHandle`]
/// backing the subscription. The caller owns the handle's lifetime: keep it
/// alive for as long as the node should keep updating, or `mem::forget` it
/// (as a whole-process root mount does) to make the leak an explicit choice
/// rather than the library's default.
pub fn reactive_text(
    document: &Document,
    signal: appfront_core::Signal<String>,
) -> Result<(Node, appfront_core::EffectHandle), wasm_bindgen::JsValue> {
    let text_node = document.create_text_node(&signal.get());
    let node_for_effect = text_node.clone();
    let handle = appfront_core::create_effect(move || {
        node_for_effect.set_data(&signal.get());
    });
    Ok((text_node.into(), handle))
}

// ---------------------------------------------------------------------------
// Hydration (resumability from server-rendered HTML)
// ---------------------------------------------------------------------------

/// Resume interactivity on a DOM tree that was pre-rendered by the server.
///
/// 1. Reads the serialised [`HydrationPayload`] from
///    `<script id="__APPFRONT_STATE__">`.
/// 2. Restores signal hydration state so that `Signal::hydrated("name", def)`
///    calls pick up the server-side values.
/// 3. Walks the deserialised `UITree` and attaches event listeners to
///    existing DOM elements matched by `data-appfront-id`.
///
/// No DOM nodes are created or moved — only event handlers are attached.
pub fn hydrate<Msg>(
    container: &Element,
    dispatch: Rc<dyn Fn(Msg)>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + serde::de::DeserializeOwned + 'static,
{
    let Some(payload) = read_state_payload::<Msg>()? else {
        // No hydration state found — nothing to resume.
        return Ok(());
    };

    // Restore signal values so Signal::hydrated(...) picks them up.
    appfront_core::set_hydration_state(payload.signals);

    let id_map = build_id_map(container)?;
    hydrate_node(&payload.tree, &dispatch, &id_map)?;

    Ok(())
}

/// Read and deserialise `<script id="__APPFRONT_STATE__">` if it exists.
fn read_state_payload<Msg>() -> Result<Option<HydrationPayload<Msg>>, wasm_bindgen::JsValue>
where
    Msg: serde::de::DeserializeOwned,
{
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");

    let Some(script_el) = document.get_element_by_id("__APPFRONT_STATE__") else {
        return Ok(None);
    };

    let json = script_el.text_content().unwrap_or_default();
    let payload: HydrationPayload<Msg> = serde_json::from_str(&json).map_err(|e| {
        wasm_bindgen::JsValue::from_str(&format!("failed to parse __APPFRONT_STATE__: {e}"))
    })?;

    Ok(Some(payload))
}

/// Collect every element with a `data-appfront-id` attribute into a
/// `u64 -> Element` map for O(1) lookups during hydration.
fn build_id_map(container: &Element) -> Result<HashMap<u64, Element>, wasm_bindgen::JsValue> {
    let list = container.query_selector_all("[data-appfront-id]")?;
    let mut map = HashMap::new();
    for i in 0..list.length() {
        let node = list.item(i);
        if let Some(el) = node.and_then(|n| n.dyn_into::<Element>().ok()) {
            if let Some(id_str) = el.get_attribute("data-appfront-id") {
                if let Ok(id) = id_str.parse::<u64>() {
                    map.insert(id, el.clone());
                }
            }
        }
    }
    Ok(map)
}

/// Recursively walk the `UITree` and attach listeners to pre-existing DOM
/// elements whose `data-appfront-id` matches.
fn hydrate_node<Msg>(
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
    id_map: &HashMap<u64, Element>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    if let Some(id) = ui.meta.data_appfront_id {
        if let Some(el) = id_map.get(&id) {
            attach_listeners(ui, dispatch, el)?;
        }
    }

    match &ui.kind {
        NodeKind::Container { children } => {
            for child in children {
                hydrate_node(child, dispatch, id_map)?;
            }
        }
        NodeKind::List { items } => {
            for item in items {
                hydrate_node(item, dispatch, id_map)?;
            }
        }
        NodeKind::DataGrid { .. }
        | NodeKind::Heading { .. }
        | NodeKind::Text { .. }
        | NodeKind::Button { .. }
        | NodeKind::Input { .. } => {}
    }

    Ok(())
}

/// Attach event listeners (and AI attributes) to a single existing element.
fn attach_listeners<Msg>(
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
    el: &Element,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    if let Some(msg) = ui.meta.on_click.clone() {
        let dispatch = Rc::clone(dispatch);
        let closure = Closure::<dyn FnMut()>::new(move || dispatch(msg.clone()));
        let html_el: &web_sys::HtmlElement = el.dyn_ref::<web_sys::HtmlElement>().ok_or_else(|| {
            wasm_bindgen::JsValue::from_str("element is not an HtmlElement")
        })?;
        html_el.set_onclick(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }

    if let Some(action) = &ui.meta.ai.action {
        el.set_attribute("data-ai-action", action)?;
        if !ui.meta.ai.params.is_empty() {
            let params_json = serde_json::to_string(&json_obj(&ui.meta.ai.params))
                .unwrap_or_default();
            el.set_attribute("data-ai-params", &params_json)?;
        }
    }

    Ok(())
}
