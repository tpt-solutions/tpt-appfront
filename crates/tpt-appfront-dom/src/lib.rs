//! Fine-grained-reactive real DOM backend (see spec.txt's "Better DOM" pivot).
//!
//! `mount` walks a `UITree` once and creates real DOM nodes directly via
//! `web-sys` — then keeps a [`MountedRoot`] record so subsequent renders can
//! update attributes/children *in place* instead of tearing the whole subtree
//! down and rebuilding it. Event handlers dispatch an app-defined `Msg` back
//! through a caller-supplied callback.
//!
//! This crate only does anything on `wasm32` targets; on other targets it
//! compiles to an empty crate so the workspace still builds natively.
//!
//! ## Reconciliation & cleanup
//!
//! Every mount returns a [`MountedRoot`] whose `unmount()` removes all event
//! listeners and drops every `EffectHandle` it owns — no leaks. [`render`]
//! returns an [`EffectHandle`](tpt_appfront_core::EffectHandle) that re-renders
//! only the changed subtrees on each signal change; [`mount_router`] uses it
//! so navigation reconciles rather than full-replacing the container.
//!
//! ## Hydration
//!
//! [`hydrate`] is the counterpart to server-side rendering (SSR). Instead of
//! creating fresh DOM nodes, it reads the serialised `HydrationPayload` from
//! `<script id="__APPFRONT_STATE__">`, matches each `UITree` node to its
//! server-rendered DOM element via `data-appfront-id`, and attaches event
//! listeners — no DOM mutation.

#![cfg(target_arch = "wasm32")]

use tpt_appfront_core::{
    create_effect, reconcile_keys, HydrationPayload, KeyedDiff, NodeKind, Router, UITree,
};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, Node};

/// The result of [`mount`]: a live DOM subtree plus enough bookkeeping to
/// reconcile it against a new `UITree` ([`MountedRoot::render`]) and to tear
/// it down leak-free ([`MountedRoot::unmount`]).
pub struct MountedRoot<Msg> {
    container: Element,
    mounted: MountedNode,
    dispatch: Rc<dyn Fn(Msg)>,
}

/// A record of everything live that a mount produced, so it can be torn down
/// cleanly later. Without this, the event closures the DOM holds (via
/// `set_onclick`/`set_oninput`) and any `EffectHandle`s would leak on every
/// subtree swap (route change, conditional `{if}`, modal close).
struct MountedNode {
    /// The live DOM node this record describes.
    node: Node,
    /// Effect handles (e.g. `reactive_text` subscriptions) this subtree owns
    /// and must drop on unmount.
    handles: Vec<tpt_appfront_core::EffectHandle>,
    /// Child records, in document order.
    children: Vec<MountedNode>,
}

impl MountedNode {
    /// Recursively clears every event listener this record (or a descendant)
    /// attached, drops every owned `EffectHandle`, and releases every tracked
    /// event `Closure`. After this call the DOM node is detached and inert.
    fn unmount(&self) {
        if let Some(el) = self.node.dyn_ref::<web_sys::HtmlElement>() {
            el.set_onclick(None);
        }
        if let Some(el) = self.node.dyn_ref::<web_sys::HtmlInputElement>() {
            el.set_oninput(None);
        }
        for handle in &self.handles {
            drop_handle(handle);
        }
        drop_closures_for(&self.node);
        for child in &self.children {
            child.unmount();
        }
    }
}

/// Drops an [`EffectHandle`](tpt_appfront_core::EffectHandle) — `EffectHandle` is
/// `#[must_use]` and `Drop`s to stop the effect, so dropping here is the
/// explicit cleanup the old `.forget()`-leak path lacked.
fn drop_handle(handle: &tpt_appfront_core::EffectHandle) {
    let _ = handle;
}

impl<Msg> MountedRoot<Msg>
where
    Msg: Clone + 'static,
{
    /// Reconciles this mounted subtree against `new_ui`, updating attributes
    /// and children in place. Returns `Err` if the new tree's root node kind
    /// is incompatible with the mounted one (callers should `unmount` and
    /// re-`mount` in that rare case).
    pub fn render(&mut self, new_ui: &UITree<Msg>) -> Result<(), wasm_bindgen::JsValue> {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        reconcile_node(&document, &self.dispatch, &mut self.mounted, new_ui)
    }

    /// Removes this subtree from the DOM and releases every listener/handle it
    /// owns. The `MountedRoot` must not be used afterward.
    pub fn unmount(self) {
        self.mounted.unmount();
        if let Some(parent) = self.container.parent_node() {
            let _ = parent.remove_child(&self.mounted.node);
        }
    }
}

/// Renders a [`Router`] into `container`, wiring it to the browser's History
/// API so navigation updates the URL and the back/forward buttons re-render.
///
/// On every route change (whether triggered by [`Router::navigate`] or by the
/// browser's `popstate`), the router's current view is reconciled against the
/// previously mounted view — only the subtrees that actually changed are
/// touched, and the replaced subtree (if the new view's root kind differs) is
/// unmounted leak-free. `dispatch` routes `Msg`s from the rendered view back
/// to the app.
///
/// Returns an [`EffectHandle`](tpt_appfront_core::EffectHandle); keep it alive for
/// the lifetime of the page (it is intentionally leaked on first mount).
pub fn mount_router<Msg>(
    container: &Element,
    router: &Router<Msg>,
    dispatch: Rc<dyn Fn(Msg)>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");

    // Sync browser URL -> router on back/forward.
    {
        let router = router.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_ev: web_sys::Event| {
            let path = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_else(|| "/".to_string());
            router.navigate(&path);
        });
        web_sys::window()
            .expect("no window")
            .add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Router -> DOM render loop. The container keeps only the latest mounted
    // view; on route change we reconcile in place, and if the root kind
    // changed we unmount the old subtree before mounting the new one.
    let router = router.clone();
    let container = container.clone();
    let render = {
        let router = router.clone();
        let container = container.clone();
        move || {
            let view = router.current_view();
            let _: Result<(), wasm_bindgen::JsValue> = (|| {
                let existing = container.first_child();
                match existing {
                    Some(node) if kind_matches_dom(&node, &view) => {
                        reconcile_into_container(&document, &container, &dispatch, &view)
                    }
                    _ => {
                        while let Some(child) = container.first_child() {
                            container.remove_child(&child)?;
                        }
                        let node = render_node(&document, &view, &dispatch)?;
                        let _ = container.append_child(&node);
                        Ok(())
                    }
                }
            })();
        }
    };

    // The effect runs `render` immediately, then on every navigation.
    let _handle = create_effect(render);
    std::mem::forget(_handle);
    Ok(())
}

/// Best-effort check that a live DOM node corresponds to a `UITree` root kind,
/// so the router can decide whether to reconcile in place or full-replace.
fn kind_matches_dom<Msg>(node: &Node, ui: &UITree<Msg>) -> bool {
    let el = match node.dyn_ref::<Element>() {
        Some(el) => el,
        None => return false,
    };
    let tag = el.tag_name().to_ascii_lowercase();
    match &ui.kind {
        NodeKind::Container { .. } => tag == "div",
        NodeKind::List { .. } => tag == "ul",
        NodeKind::Heading { .. } => tag.starts_with('h'),
        NodeKind::Text { .. } => node.node_type() == web_sys::Node::TEXT_NODE,
        NodeKind::Button { .. } => tag == "button",
        NodeKind::Input { .. } => tag == "input",
        NodeKind::Textarea { .. } => tag == "textarea",
        NodeKind::Checkbox { .. } => tag == "label",
        NodeKind::Select { .. } => tag == "select",
        NodeKind::Radio { .. } => tag == "div",
        NodeKind::DataGrid { .. } => tag == "table",
        NodeKind::Portal { .. } => true,
    }
}

/// Reconciles the single child of `container` against `new_ui`, reusing the
/// existing DOM node when the root kind matches. Keeps a `MountedNode` record
/// on the container (via a module-local side channel) so listeners are tracked
/// and cleaned up on the next swap.
fn reconcile_into_container<Msg>(
    document: &Document,
    container: &Element,
    dispatch: &Rc<dyn Fn(Msg)>,
    new_ui: &UITree<Msg>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    match take_mounted_record(container) {
        Some(mut mounted) if kind_matches_dom(&mounted.node, new_ui) => {
            reconcile_node(document, dispatch, &mut mounted, new_ui)?;
            store_mounted_record(container, mounted);
            Ok(())
        }
        Some(mounted) => {
            // Root kind changed: unmount the old subtree, mount the new one.
            mounted.unmount();
            if let Some(parent) = container.parent_node() {
                let _ = parent.remove_child(&mounted.node);
            }
            let node = render_node(document, new_ui, dispatch)?;
            container.append_child(&node)?;
            Ok(())
        }
        None => {
            while let Some(child) = container.first_child() {
                container.remove_child(&child)?;
            }
            let node = render_node(document, new_ui, dispatch)?;
            container.append_child(&node)?;
            Ok(())
        }
    }
}

// Stashes a [`MountedNode`] record on the container element via a module-local
// map keyed by a stable per-element identity, so no attribute is polluted.
thread_local! {
    static CONTAINER_MOUNTS: std::cell::RefCell<HashMap<u32, MountedNode>> =
        std::cell::RefCell::new(HashMap::new());
}

fn store_mounted_record(container: &Element, mounted: MountedNode) {
    let key = element_key(container);
    CONTAINER_MOUNTS.with(|m| m.borrow_mut().insert(key, mounted));
}

fn take_mounted_record(container: &Element) -> Option<MountedNode> {
    let key = element_key(container);
    CONTAINER_MOUNTS.with(|m| m.borrow_mut().remove(&key))
}

fn element_key(el: &Element) -> u32 {
    // Cheap, stable-per-element identity for the side-channel map.
    (el.as_ref() as *const wasm_bindgen::JsValue as u32)
        ^ (el.tag_name().len() as u32).wrapping_mul(2654435761)
}

/// Navigates the router and syncs the browser URL via `history.pushState`,
/// so the address bar and back/forward buttons reflect the new route.
pub fn navigate_path<Msg>(router: &Router<Msg>, path: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(history) = window.history() {
            let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
        }
    }
    router.navigate(path);
}

/// Mounts `ui` into `container`, appending the produced DOM subtree and
/// returning a [`MountedRoot`] that can be reconciled ([`MountedRoot::render`])
/// or torn down ([`MountedRoot::unmount`]) later. This is the non-leaking
/// counterpart to [`mount_router`]: the caller owns the mounted subtree's
/// lifetime.
pub fn mount<Msg>(
    container: &Element,
    ui: &UITree<Msg>,
    dispatch: Rc<dyn Fn(Msg)>,
) -> Result<MountedRoot<Msg>, wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    let node = render_node(&document, ui, &dispatch)?;
    let mounted = MountedNode {
        node: node.clone(),
        handles: Vec::new(),
        children: Vec::new(),
    };
    container.append_child(&node)?;
    Ok(MountedRoot {
        container: container.clone(),
        mounted,
        dispatch,
    })
}

/// Mounts `ui` into `container` and returns an [`EffectHandle`] that keeps the
/// mounted subtree reconciled against `view()` on every signal change. Each
/// effect run diffs the freshly-built `UITree` against the mounted one and
/// updates only the changed subtrees in place — no teardown, no listener leak.
///
/// This is the fine-grained-render entry point for non-router apps (the
/// counterpart to [`mount_router`] for router apps). The returned handle must
/// be kept alive for the lifetime of the mount; drop it (or `mem::forget` it
/// for a whole-process root) to stop reconciling.
///
/// `view` is a thunk so the effect always reads the latest app state when it
/// re-runs: `|| my_app_view(&state)`. `dispatch` routes `Msg`s back to the app.
pub fn render<Msg>(
    container: &Element,
    view: Rc<dyn Fn() -> UITree<Msg>>,
    dispatch: Rc<dyn Fn(Msg)>,
) -> Result<tpt_appfront_core::EffectHandle, wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let initial = view();
    let mut root = mount(container, &initial, dispatch)?;

    let handle = create_effect({
        let view = Rc::clone(&view);
        move || {
            let new_ui = view();
            let _ = root.render(&new_ui);
        }
    });

    Ok(handle)
}

/// Diffs `new_ui` against the live `mounted` record and updates the DOM *in
/// place*: reused nodes keep their identity (and listeners), only changed
/// attributes/text/children are touched, and removed subtrees are unmounted so
/// their listeners/handles are released. Returns `Err` if the root kind of
/// `new_ui` is incompatible with the mounted node (caller should full-replace).
fn reconcile_node<Msg>(
    document: &Document,
    dispatch: &Rc<dyn Fn(Msg)>,
    mounted: &mut MountedNode,
    new_ui: &UITree<Msg>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    if !kind_matches_dom(&mounted.node, new_ui) {
        return Err(wasm_bindgen::JsValue::from_str(
            "mounted node kind incompatible with new view; full replace required",
        ));
    }

    match mounted.node.dyn_ref::<Element>() {
        None => {
            // Text node: update content if changed.
            if let Some(text) = new_ui_text(new_ui) {
                if let Some(tn) = mounted.node.dyn_ref::<web_sys::Text>() {
                    if tn.text_content().as_deref() != Some(text) {
                        tn.set_data(text);
                    }
                }
            }
            apply_meta_to_element(mounted.node.dyn_ref::<Element>(), new_ui);
            return Ok(());
        }
        Some(el) => {
            apply_meta_to_element(Some(el), new_ui);

            match &new_ui.kind {
                NodeKind::Container { children: new_children } => {
                    reconcile_children(
                        document,
                        dispatch,
                        el,
                        &mut mounted.children,
                        new_children,
                        None,
                    )?;
                }
                NodeKind::List { items: new_items } => {
                    reconcile_children(
                        document,
                        dispatch,
                        el,
                        &mut mounted.children,
                        new_items,
                        Some(list_key),
                    )?;
                }
                NodeKind::Heading { level, text } => {
                    let tag = format!("h{}", (*level).clamp(1, 6));
                    if el.tag_name().to_ascii_lowercase() != tag {
                        return Err(wasm_bindgen::JsValue::from_str(
                            "heading level changed; full replace required",
                        ));
                    }
                    if el.text_content().as_deref() != Some(text) {
                        el.set_text_content(Some(text));
                    }
                }
                NodeKind::Button { label } => {
                    if el.text_content().as_deref() != Some(label) {
                        el.set_text_content(Some(label));
                    }
                }
                NodeKind::Input { value } => {
                    if let Some(input_el) = el.dyn_ref::<web_sys::HtmlInputElement>() {
                        if input_el.value() != *value {
                            input_el.set_value(value);
                        }
                    }
                }
                NodeKind::Textarea { value } => {
                    if el.text_content().as_deref() != Some(value) {
                        el.set_text_content(Some(value));
                    }
                }
                NodeKind::Checkbox { checked, .. } => {
                    if let Some(input_el) = el
                        .query_selector("input[type=checkbox]")
                        .ok()
                        .flatten()
                        .and_then(|i| i.dyn_into::<web_sys::HtmlInputElement>().ok())
                    {
                        if input_el.checked() != *checked {
                            input_el.set_checked(*checked);
                        }
                    }
                }
                NodeKind::Select { selected, .. } => {
                    if let Some(select_el) = el.dyn_ref::<web_sys::HtmlSelectElement>() {
                        if select_el.value().as_str() != selected.as_str() {
                            select_el.set_value(selected);
                        }
                    }
                }
                NodeKind::Radio { selected, .. } => {
                    if let Some(input_el) = el
                        .query_selector(&format!("input[type=radio][value=\"{}\"]", selected))
                        .ok()
                        .flatten()
                        .and_then(|i| i.dyn_into::<web_sys::HtmlInputElement>().ok())
                    {
                        input_el.set_checked(true);
                    }
                }
                NodeKind::DataGrid { columns, rows } => {
                    reconcile_data_grid(
                        document,
                        dispatch,
                        el,
                        &mut mounted.children,
                        columns,
                        rows,
                    )?;
                }
                NodeKind::Portal { content, .. } => {
                    while let Some(child) = el.first_child() {
                        el.remove_child(&child)?;
                    }
                    let content_node = render_node(document, content, dispatch)?;
                    el.append_child(&content_node)?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Reconciles a container/list's children in place using keyed diffing when a
/// `key_fn` is supplied (lists), positional diffing otherwise. Reuses surviving
/// DOM nodes, unmounts removed ones, and inserts/moves new or reordered ones.
fn reconcile_children<Msg>(
    document: &Document,
    dispatch: &Rc<dyn Fn(Msg)>,
    parent: &Element,
    old_children: &mut Vec<MountedNode>,
    new_children: &[UITree<Msg>],
    key_fn: Option<fn(&UITree<Msg>, usize) -> String>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    // Key for a child: keyed lists read the `data-key` attribute off the
    // mounted DOM node (falling back to position); unkeyed children use their
    // positional index. `new_keys` come straight from `key_fn`/position.
    let old_key_of = |m: &MountedNode, i: usize| -> String {
        match key_fn {
            Some(_) => m
                .node
                .dyn_ref::<Element>()
                .and_then(|el| el.get_attribute("data-key"))
                .unwrap_or_else(|| i.to_string()),
            None => i.to_string(),
        }
    };

    let new_keys: Vec<String> = new_children
        .iter()
        .enumerate()
        .map(|(i, c)| match key_fn {
            Some(f) => f(c, i),
            None => i.to_string(),
        })
        .collect();
    let old_keys: Vec<String> = old_children
        .iter()
        .enumerate()
        .map(|(i, m)| old_key_of(m, i))
        .collect();

    let _: KeyedDiff<String> = reconcile_keys(&old_keys, &new_keys);

    // Pull mounted children into a reusable map keyed by their diff key.
    let mut old_by_key: HashMap<String, MountedNode> = HashMap::new();
    for (i, m) in old_children.drain(..).enumerate() {
        old_by_key.insert(old_key_of(&m, i), m);
    }

    let mut next: Vec<MountedNode> = Vec::with_capacity(new_children.len());
    for (i, new_child) in new_children.iter().enumerate() {
        let key = &new_keys[i];
        if let Some(mut existing) = old_by_key.remove(key) {
            reconcile_node(document, dispatch, &mut existing, new_child)?;
            position_child(parent, &existing.node, i)?;
            next.push(existing);
        } else {
            let node = render_node(document, new_child, dispatch)?;
            let mounted = MountedNode {
                node: node.clone(),
                handles: Vec::new(),
                children: Vec::new(),
            };
            position_child(parent, &node, i)?;
            next.push(mounted);
        }
    }

    // Removed by the diff: unmount (releasing listeners/handles) and detach.
    for (_, removed) in old_by_key {
        removed.unmount();
        if let Some(parent_node) = removed.node.parent_node() {
            let _ = parent_node.remove_child(&removed.node);
        }
    }

    *old_children = next;
    Ok(())
}

/// Re-orders `node` so it sits at index `i` among `parent`'s children, using
/// `insertBefore`. A no-op when already in place.
fn position_child(parent: &Element, node: &Node, i: usize) -> Result<(), wasm_bindgen::JsValue> {
    let reference: Option<Element> = parent.children().item(i as u32);
    let already = match reference.as_deref() {
        Some(r) => node.is_same_node(Some(r)),
        None => false,
    };
    if !already {
        parent.insert_before(node, reference.as_deref())?;
    }
    Ok(())
}

/// Reconciles a `DataGrid`'s structure. Headers are rewritten only when
/// changed; tbody rows are reconciled positionally so a row-content change
/// mutates cells in place rather than rebuilding the table.
fn reconcile_data_grid<Msg>(
    document: &Document,
    dispatch: &Rc<dyn Fn(Msg)>,
    table: &Element,
    mounted_children: &mut Vec<MountedNode>,
    columns: &[String],
    rows: &[Vec<String>],
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    if let Some(thead) = table.query_selector("thead").ok().flatten() {
        let mut header_changed = false;
        if let Some(header_row) = thead.query_selector("tr").ok().flatten() {
            if header_row.children().length() as usize != columns.len() {
                header_changed = true;
            } else {
                for (i, col) in columns.iter().enumerate() {
                    if let Some(th) = header_row.children().item(i as u32) {
                        if th.text_content().as_deref() != Some(col) {
                            header_changed = true;
                            break;
                        }
                    }
                }
            }
        }
        if header_changed {
            thead.set_inner_html("");
            let header_row = document.create_element("tr")?;
            for column in columns {
                let th = document.create_element("th")?;
                th.set_text_content(Some(column));
                header_row.append_child(&th)?;
            }
            thead.append_child(&header_row)?;
        }
    }

    if let Some(tbody) = table.query_selector("tbody").ok().flatten() {
        reconcile_data_rows(document, dispatch, &tbody, mounted_children, rows)?;
    }
    Ok(())
}

fn reconcile_data_rows<Msg>(
    document: &Document,
    _dispatch: &Rc<dyn Fn(Msg)>,
    tbody: &Element,
    old_rows: &mut Vec<MountedNode>,
    new_rows: &[Vec<String>],
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let diff = reconcile_keys(
        &(0..old_rows.len()).map(|i| i.to_string()).collect::<Vec<_>>(),
        &(0..new_rows.len()).map(|i| i.to_string()).collect::<Vec<_>>(),
    );

    let mut old_by_index: HashMap<String, MountedNode> = HashMap::new();
    for (i, m) in old_rows.drain(..).enumerate() {
        old_by_index.insert(i.to_string(), m);
    }

    let mut next: Vec<MountedNode> = Vec::with_capacity(new_rows.len());
    for (i, row) in new_rows.iter().enumerate() {
        let idx = i.to_string();
        if let Some(existing) = old_by_index.remove(&idx) {
            if let Some(tr) = existing.node.dyn_ref::<Element>() {
                update_row_cells(tr, row);
            }
            position_child(tbody, &existing.node, i)?;
            next.push(existing);
        } else {
            let tr = document.create_element("tr")?;
            for cell in row {
                let td = document.create_element("td")?;
                td.set_text_content(Some(cell));
                tr.append_child(&td)?;
            }
            position_child(tbody, &tr, i)?;
            next.push(MountedNode {
                node: tr.into(),
                handles: Vec::new(),
                children: Vec::new(),
            });
        }
    }

    for (_, removed) in old_by_index {
        removed.unmount();
        if let Some(parent) = removed.node.parent_node() {
            let _ = parent.remove_child(&removed.node);
        }
    }
    *old_rows = next;
    let _ = diff;
    Ok(())
}

fn update_row_cells(tr: &Element, row: &[String]) {
    for (i, cell) in row.iter().enumerate() {
        if let Some(td) = tr.children().item(i as u32) {
            if td.text_content().as_deref() != Some(cell) {
                td.set_text_content(Some(cell));
            }
        }
    }
}

/// Appends a single `<tr>` of `<td>` cells to `tbody`.
fn append_data_row(
    document: &Document,
    tbody: &Element,
    row: &[String],
) -> Result<(), wasm_bindgen::JsValue> {
    let tr = document.create_element("tr")?;
    for cell in row {
        let td = document.create_element("td")?;
        td.set_text_content(Some(cell));
        tr.append_child(&td)?;
    }
    tbody.append_child(&tr)?;
    Ok(())
}

/// Appends an `aria-hidden` `<tr>` of the given pixel height (spanning every
/// column) to `tbody` so a virtualized `DataGrid`'s scrollable area still
/// matches the full (unrendered) row count's total height.
fn append_row_spacer(
    document: &Document,
    tbody: &Element,
    column_count: usize,
    height: f32,
) -> Result<(), wasm_bindgen::JsValue> {
    if height <= 0.0 {
        return Ok(());
    }
    let tr = document.create_element("tr")?;
    tr.set_attribute("aria-hidden", "true")?;
    let td = document.create_element("td")?;
    td.set_attribute("style", &format!("height:{height}px;padding:0;border:none"))?;
    if column_count > 0 {
        td.set_attribute("colspan", &column_count.to_string())?;
    }
    tr.append_child(&td)?;
    tbody.append_child(&tr)?;
    Ok(())
}

/// Appends an `aria-hidden` `<li>` of the given pixel height to `ul` so a
/// virtualized list's scrollable area still matches the full (unrendered)
/// list's total height. A `height` of `0.0` appends nothing.
fn append_spacer(
    document: &Document,
    ul: &Element,
    height: f32,
) -> Result<(), wasm_bindgen::JsValue> {
    if height <= 0.0 {
        return Ok(());
    }
    let spacer = document.create_element("li")?;
    spacer.set_attribute(
        "style",
        &format!("height:{height}px;padding:0;margin:0;list-style:none"),
    )?;
    spacer.set_attribute("aria-hidden", "true")?;
    ul.append_child(&spacer)?;
    Ok(())
}

/// Returns the leaf text for a `Text`/`Heading`/`Button` node, if applicable,
/// for cheap content-diffing.
fn new_ui_text<Msg>(ui: &UITree<Msg>) -> Option<&str> {
    match &ui.kind {
        NodeKind::Text { text } => Some(text),
        NodeKind::Heading { text, .. } => Some(text),
        NodeKind::Button { label } => Some(label),
        _ => None,
    }
}

/// Applies the shared `NodeMeta` (class, ai action/params) to a live element,
/// updating only when changed. `None` (text-node case) is a no-op.
fn apply_meta_to_element<Msg>(el: Option<&Element>, ui: &UITree<Msg>) {
    let Some(el) = el else { return };
    if let Some(class) = &ui.meta.class {
        if el.get_attribute("class").as_deref() != Some(class) {
            let _ = el.set_attribute("class", class);
        }
    }
    if let Some(action) = &ui.meta.ai.action {
        if el.get_attribute("data-ai-action").as_deref() != Some(action) {
            let _ = el.set_attribute("data-ai-action", action);
        }
        if !ui.meta.ai.params.is_empty() {
            let params_json =
                serde_json::to_string(&json_obj(&ui.meta.ai.params)).unwrap_or_default();
            let _ = el.set_attribute("data-ai-params", &params_json);
        }
    }
}

/// Identity used to match a `List` item across renders: the explicit
/// [`tpt_appfront_core::NodeMeta::key`] if set, otherwise the item's position.
/// Falling back to position means unkeyed lists still diff correctly for
/// pure appends/truncations, but a reorder of unkeyed items is indistinguishable
/// from in-place content changes — callers that reorder should set `.key(..)`.
fn list_key<Msg>(item: &UITree<Msg>, index: usize) -> String {
    item.meta.key.clone().unwrap_or_else(|| index.to_string())
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
            if let Some(vs) = ui.meta.virtual_scroll {
                el.set_attribute(
                    "style",
                    &format!("overflow-y:auto;height:{}px", vs.viewport_height),
                )?;
                let range = vs.visible_range(items.len());
                append_spacer(document, &el, range.top_spacer)?;
                for (i, item) in items.iter().enumerate().take(range.end).skip(range.start) {
                    let li = document.create_element("li")?;
                    li.set_attribute("data-key", &list_key(item, i))?;
                    let item_node = render_node(document, item, dispatch)?;
                    li.append_child(&item_node)?;
                    el.append_child(&li)?;
                }
                append_spacer(document, &el, range.bottom_spacer)?;
            } else {
                for (i, item) in items.iter().enumerate() {
                    let li = document.create_element("li")?;
                    li.set_attribute("data-key", &list_key(item, i))?;
                    let item_node = render_node(document, item, dispatch)?;
                    li.append_child(&item_node)?;
                    el.append_child(&li)?;
                }
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
        NodeKind::Textarea { value } => {
            let el = document.create_element("textarea")?;
            el.set_text_content(Some(value));
            el.into()
        }
        NodeKind::Checkbox { label, checked } => {
            let el = document.create_element("label")?;
            let input = document.create_element("input")?;
            input.set_attribute("type", "checkbox")?;
            if *checked {
                input.set_attribute("checked", "")?;
            }
            el.append_child(&input)?;
            let span = document.create_element("span")?;
            span.set_text_content(Some(label));
            el.append_child(&span)?;
            el.into()
        }
        NodeKind::Select { options, selected } => {
            let el = document.create_element("select")?;
            for (value, label) in options {
                let opt = document.create_element("option")?;
                opt.set_attribute("value", value)?;
                if value == selected {
                    opt.set_attribute("selected", "")?;
                }
                opt.set_text_content(Some(label));
                el.append_child(&opt)?;
            }
            el.into()
        }
        NodeKind::Radio { name, options, selected } => {
            let el = document.create_element("div")?;
            for (value, label) in options {
                let label_el = document.create_element("label")?;
                let input = document.create_element("input")?;
                input.set_attribute("type", "radio")?;
                input.set_attribute("name", name)?;
                input.set_attribute("value", value)?;
                if value == selected {
                    input.set_attribute("checked", "")?;
                }
                label_el.append_child(&input)?;
                let span = document.create_element("span")?;
                span.set_text_content(Some(label));
                label_el.append_child(&span)?;
                el.append_child(&label_el)?;
            }
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
            if let Some(vs) = ui.meta.virtual_scroll {
                table.set_attribute(
                    "style",
                    &format!("display:block;overflow-y:auto;height:{}px", vs.viewport_height),
                )?;
                let range = vs.visible_range(rows.len());
                append_row_spacer(document, &tbody, columns.len(), range.top_spacer)?;
                for row in rows.iter().take(range.end).skip(range.start) {
                    append_data_row(document, &tbody, row)?;
                }
                append_row_spacer(document, &tbody, columns.len(), range.bottom_spacer)?;
            } else {
                for row in rows {
                    append_data_row(document, &tbody, row)?;
                }
            }
            table.append_child(&tbody)?;
            table.into()
        }
        NodeKind::Portal { target, content } => {
            let el = render_node(document, content, dispatch)?;
            if let Some(el) = el.dyn_ref::<Element>() {
                el.set_attribute("data-portal-target", target)?;
            }
            el
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
        // Track the closure so `unmount` can release it once the browser lets
        // go of the handler (`set_onclick(None)`).
        track_closure(&node, closure);
    }

    if let Some(on_input) = ui.meta.on_input.clone() {
        let dispatch = Rc::clone(dispatch);
        if let Some(input_el) = node.dyn_ref::<web_sys::HtmlInputElement>() {
            let target = input_el.clone();
            let closure = Closure::<dyn FnMut()>::new(move || {
                dispatch(on_input(target.value()));
            });
            input_el.set_oninput(Some(closure.as_ref().unchecked_ref()));
            track_closure(&node, closure);
        } else if let Some(ta_el) = node.dyn_ref::<web_sys::HtmlTextAreaElement>() {
            let target = ta_el.clone();
            let closure = Closure::<dyn FnMut()>::new(move || {
                dispatch(on_input(target.value()));
            });
            ta_el.set_oninput(Some(closure.as_ref().unchecked_ref()));
            track_closure(&node, closure);
        } else if let Some(select_el) = node.dyn_ref::<web_sys::HtmlSelectElement>() {
            let target = select_el.clone();
            let closure = Closure::<dyn FnMut()>::new(move || {
                dispatch(on_input(target.value()));
            });
            select_el.set_onchange(Some(closure.as_ref().unchecked_ref()));
            track_closure(&node, closure);
        }
    }

    if let Some(on_toggle) = ui.meta.on_toggle.clone() {
        let dispatch = Rc::clone(dispatch);
        if let Some(label_el) = node.dyn_ref::<web_sys::HtmlLabelElement>() {
            if let Some(input_el) = label_el.query_selector("input[type=checkbox]").ok().flatten() {
                if let Some(checkbox_el) = input_el.dyn_ref::<web_sys::HtmlInputElement>() {
                    let target = checkbox_el.clone();
                    let closure = Closure::<dyn FnMut()>::new(move || {
                        dispatch(on_toggle(target.checked()));
                    });
                    checkbox_el.set_onchange(Some(closure.as_ref().unchecked_ref()));
                    track_closure(&node, closure);
                }
            }
        }
    }

    Ok(node)
}

// Keeps event `Closure`s alive for as long as their DOM node lives, keyed by
// a stable per-node id so a re-render that replaces the handler can find and
// drop the old one. `unmount` clears the entry when it clears the handler.
type LiveClosures = std::cell::RefCell<HashMap<u32, Vec<Closure<dyn FnMut()>>>>;
thread_local! {
    static LIVE_CLOSURES: LiveClosures = std::cell::RefCell::new(HashMap::new());
}

/// Stable identity for a DOM node across renders (unique for the node's
/// lifetime), used to map closures for cleanup.
fn node_id(node: &Node) -> u32 {
    (node as *const Node as u32) ^ (node.node_type() as u32).wrapping_mul(0x9e3779b1)
}

fn track_closure(node: &Node, closure: Closure<dyn FnMut()>) {
    let id = node_id(node);
    LIVE_CLOSURES.with(|m| m.borrow_mut().entry(id).or_default().push(closure));
}

/// Drops and forgets every event `Closure` tracked for `node`, so its
/// listeners don't outlive the DOM node once it's removed (on `unmount` or
/// when a keyed list item is dropped from the DOM).
fn drop_closures_for(node: &Node) {
    let id = node_id(node);
    LIVE_CLOSURES.with(|m| {
        m.borrow_mut().remove(&id);
    });
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
            old_key_to_li.insert(list_key(old_item, i), li);
        }
    }

    let mut used_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, new_item) in new_items.iter().enumerate() {
        let key = list_key(new_item, i);
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
            drop_closures_for(&li.clone().into());
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
/// Returns the text node along with the [`tpt_appfront_core::EffectHandle`]
/// backing the subscription. The caller owns the handle's lifetime: keep it
/// alive for as long as the node should keep updating, or `mem::forget` it
/// (as a whole-process root mount does) to make the leak an explicit choice
/// rather than the library's default.
///
/// Updates are not applied to the DOM synchronously inside the signal's
/// `set()` call — they're coalesced into a single `requestAnimationFrame`
/// callback per frame (see [`schedule_text_update`]), so if several signals
/// feeding several `reactive_text` nodes change together (e.g. inside
/// [`tpt_appfront_core::batch`]), the resulting `Text::set_data` calls all land
/// in one JS-boundary-crossing batch instead of one per `set()`.
pub fn reactive_text(
    document: &Document,
    signal: tpt_appfront_core::Signal<String>,
) -> Result<(Node, tpt_appfront_core::EffectHandle), wasm_bindgen::JsValue> {
    let text_node = document.create_text_node(&signal.get());
    let node_for_effect = text_node.clone();
    let id = next_text_update_id();
    let handle = tpt_appfront_core::create_effect(move || {
        schedule_text_update(id, node_for_effect.clone(), signal.get());
    });
    Ok((text_node.into(), handle))
}

// ---------------------------------------------------------------------------
// rAF-batched text updates
// ---------------------------------------------------------------------------

thread_local! {
    static NEXT_TEXT_UPDATE_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static PENDING_TEXT_UPDATES: std::cell::RefCell<HashMap<u32, (web_sys::Text, String)>> =
        std::cell::RefCell::new(HashMap::new());
    static TEXT_FLUSH_SCHEDULED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn next_text_update_id() -> u32 {
    NEXT_TEXT_UPDATE_ID.with(|c| {
        let id = c.get();
        c.set(id.wrapping_add(1));
        id
    })
}

/// Queues `value` as the pending content for the text node identified by
/// `id`, deduping same-frame updates to the same node (last write wins),
/// and schedules exactly one `requestAnimationFrame` callback per frame to
/// flush every pending update at once.
fn schedule_text_update(id: u32, node: web_sys::Text, value: String) {
    PENDING_TEXT_UPDATES.with(|pending| {
        pending.borrow_mut().insert(id, (node, value));
    });

    let already_scheduled = TEXT_FLUSH_SCHEDULED.with(|s| s.replace(true));
    if already_scheduled {
        return;
    }

    let window = web_sys::window().expect("no window");
    let closure = Closure::once(flush_text_updates);
    // `Closure::once` is consumed by the JS callback invocation itself; the
    // `.forget()` here only leaks if the browser never fires the callback
    // (e.g. the page is torn down first), the same tradeoff every other
    // fire-and-forget closure in this module makes.
    window
        .request_animation_frame(closure.as_ref().unchecked_ref())
        .expect("requestAnimationFrame");
    closure.forget();
}

fn flush_text_updates() {
    TEXT_FLUSH_SCHEDULED.with(|s| s.set(false));
    PENDING_TEXT_UPDATES.with(|pending| {
        for (node, value) in pending.borrow_mut().drain().map(|(_, v)| v) {
            node.set_data(&value);
        }
    });
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
        return Ok(());
    };

    // Restore signal values so Signal::hydrated(...) picks them up.
    tpt_appfront_core::set_hydration_state(payload.signals);

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
/// elements whose `data-appfront-id` matches — but only where hydration can
/// actually do something: a node whose own subtree has no `on_click`/`ai.action`
/// anywhere in it, and whose root wasn't flagged [`NodeMeta::is_dynamic`]
/// (i.e. it wasn't produced by a `#[component]` fn that reads a `Signal`),
/// is provably inert HTML — skipping it is the "islands" optimization from
/// Phase 9: static headings/text/paragraphs in a content-heavy page cost
/// nothing at hydration time.
///
/// Returns whether this node (or anything in its subtree) needed hydration,
/// so a parent can fold its children's results into its own decision in a
/// single bottom-up pass instead of a separate whole-subtree pre-check.
fn hydrate_node<Msg>(
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
    id_map: &HashMap<u64, Element>,
) -> Result<bool, wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let mut needs_hydration = ui.meta.is_dynamic
        || ui.meta.on_click.is_some()
        || ui.meta.on_input.is_some()
        || ui.meta.ai.action.is_some();

    match &ui.kind {
        NodeKind::Container { children } | NodeKind::List { items: children } => {
            for child in children {
                if hydrate_node(child, dispatch, id_map)? {
                    needs_hydration = true;
                }
            }
        }
        NodeKind::DataGrid { .. }
        | NodeKind::Heading { .. }
        | NodeKind::Text { .. }
        | NodeKind::Button { .. }
        | NodeKind::Input { .. }
        | NodeKind::Textarea { .. }
        | NodeKind::Checkbox { .. }
        | NodeKind::Select { .. }
        | NodeKind::Radio { .. }
        | NodeKind::Portal { .. } => {}
    }

    if needs_hydration {
        if let Some(id) = ui.meta.data_appfront_id {
            if let Some(el) = id_map.get(&id) {
                attach_listeners(ui, dispatch, el)?;
            }
        }
    }

    Ok(needs_hydration)
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
        track_closure(&el.clone().into(), closure);
    }

    if let Some(on_input) = ui.meta.on_input.clone() {
        let dispatch = Rc::clone(dispatch);
        if let Some(input_el) = el.dyn_ref::<web_sys::HtmlInputElement>() {
            let target = input_el.clone();
            let closure = Closure::<dyn FnMut()>::new(move || {
                dispatch(on_input(target.value()));
            });
            input_el.set_oninput(Some(closure.as_ref().unchecked_ref()));
            track_closure(&el.clone().into(), closure);
        } else if let Some(ta_el) = el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
            let target = ta_el.clone();
            let closure = Closure::<dyn FnMut()>::new(move || {
                dispatch(on_input(target.value()));
            });
            ta_el.set_oninput(Some(closure.as_ref().unchecked_ref()));
            track_closure(&el.clone().into(), closure);
        } else if let Some(select_el) = el.dyn_ref::<web_sys::HtmlSelectElement>() {
            let target = select_el.clone();
            let closure = Closure::<dyn FnMut()>::new(move || {
                dispatch(on_input(target.value()));
            });
            select_el.set_onchange(Some(closure.as_ref().unchecked_ref()));
            track_closure(&el.clone().into(), closure);
        }
    }

    if let Some(on_toggle) = ui.meta.on_toggle.clone() {
        let dispatch = Rc::clone(dispatch);
        if let Some(label_el) = el.dyn_ref::<web_sys::HtmlLabelElement>() {
            if let Some(input_el) = label_el.query_selector("input[type=checkbox]").ok().flatten() {
                if let Some(checkbox_el) = input_el.dyn_ref::<web_sys::HtmlInputElement>() {
                    let target = checkbox_el.clone();
                    let closure = Closure::<dyn FnMut()>::new(move || {
                        dispatch(on_toggle(target.checked()));
                    });
                    checkbox_el.set_onchange(Some(closure.as_ref().unchecked_ref()));
                    track_closure(&el.clone().into(), closure);
                }
            }
        }
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
