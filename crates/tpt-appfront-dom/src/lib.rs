//! Fine-grained-reactive real DOM backend (see spec.txt's "Better DOM" pivot).
//!
//! `mount` walks a `UITree` once and creates real DOM nodes directly via
//! `web-sys` — then keeps a `MountedRoot` record so subsequent renders can
//! update attributes/children *in place* instead of tearing the whole subtree
//! down and rebuilding it. Event handlers dispatch an app-defined `Msg` back
//! through a caller-supplied callback.
//!
//! This crate only does anything on `wasm32` targets; on other targets it
//! compiles to an empty crate so the workspace still builds natively.
//!
//! ## Reconciliation & cleanup
//!
//! Every mount returns a `MountedRoot` whose `unmount()` removes all event
//! listeners and drops every `EffectHandle` it owns — no leaks. `render`
//! returns an [`EffectHandle`](tpt_appfront_core::EffectHandle) that re-renders
//! only the changed subtrees on each signal change; `mount_router` uses it
//! so navigation reconciles rather than full-replacing the container.
//!
//! ## Hydration
//!
//! `hydrate` is the counterpart to server-side rendering (SSR). Instead of
//! creating fresh DOM nodes, it reads the serialised `HydrationPayload` from
//! `<script id="__APPFRONT_STATE__">`, matches each `UITree` node to its
//! server-rendered DOM element via `data-appfront-id`, and attaches event
//! listeners — no DOM mutation.

#![cfg(target_arch = "wasm32")]

use std::collections::HashMap;
use std::rc::Rc;

use tpt_appfront_core::ui_tree::{
    MediaType, OnInput, OnKeyDown, OnPaste, OnPointerDown, OnPointerMove, OnPointerUp, OnToggle,
};
use tpt_appfront_core::{
    apply_auto_virtual_scroll, create_effect, reconcile_keys, HydrationPayload, KeyEvent,
    KeyedDiff, NodeKind, NodeMeta, PointerEvent, Router, UITree,
};
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
    /// The diff key this node was last reconciled under (a list item's key). Kept here so a
    /// re-render compares keys without reading `data-key` back from the DOM.
    key: Option<String>,
    /// `presence_mask` of the handlers this node was mounted or last reconciled with.
    handlers: u32,
    /// `subtree_hash` of the subtree as last reconciled, or `None` when it is not skippable.
    hash: Option<u64>,
}

impl MountedNode {
    /// Recursively clears every event listener this record (or a descendant)
    /// attached, drops every owned `EffectHandle`, and releases every tracked
    /// event `Closure`. After this call the DOM node is detached and inert.
    fn unmount(&self) {
        if let Some(el) = self.node.dyn_ref::<web_sys::HtmlElement>() {
            el.set_onclick(None);
            el.set_onfocus(None);
            el.set_onblur(None);
            el.set_onkeydown(None);
            el.set_onpointerdown(None);
            el.set_onpointermove(None);
            el.set_onpointerup(None);
            el.set_onpaste(None);
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
        let document = document()?;
        reconcile_node(&document, &self.dispatch, &mut self.mounted, new_ui)
    }

    /// Removes this subtree from the DOM and releases every listener/handle it
    /// owns. The `MountedRoot` must not be used afterward.
    pub fn unmount(self) {
        self.mounted.unmount();
        // `self.container` is `self.mounted.node`'s actual parent (`mount`
        // appends into it directly: `container.append_child(&node)`) — not
        // `self.container`'s *own* parent, which is what `.parent_node()`
        // was reaching for. That mismatch made `remove_child` throw
        // (silently, via the `let _ =` that already existed here) because
        // `self.mounted.node` was never a child of that grandparent
        // element, so the old root was never actually detached. Every
        // caller that unmounts-then-remounts into the same container
        // (`render_with`'s kind-mismatch fallback, a router's route
        // change) ended up *appending* a second full copy of the tree
        // next to the still-attached "old" one instead of replacing it.
        let _ = self.container.remove_child(&self.mounted.node);
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
    mount_router_with(container, router, dispatch, false)
}

/// Like [`mount_router`], but with `auto_optimize` enabled: large unconfigured
/// `List`/`DataGrid` nodes are windowed automatically on every route render
/// (canvas parity via [`apply_auto_virtual_scroll`]).
pub fn mount_router_with<Msg>(
    container: &Element,
    router: &Router<Msg>,
    dispatch: Rc<dyn Fn(Msg)>,
    auto_optimize: bool,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    // Fail early (before registering anything) when there is no window or document.
    let document = document()?;

    // Sync browser URL -> router on back/forward.
    {
        let router = router.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_ev: web_sys::Event| {
            let path = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_else(|| "/".to_string());
            router.navigate(&path);
        });
        window()?.add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref())?;
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
            let mut view = router.current_view();
            if auto_optimize {
                let h = container.client_height() as f32;
                apply_auto_virtual_scroll(&mut view, h);
            }
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
    // Text nodes are not Elements — check the node type directly, otherwise
    // every tree with `Text` children would fail reconciliation here and the
    // in-place text update in `reconcile_node` would be unreachable.
    if matches!(ui.kind, NodeKind::Text { .. }) {
        return node.node_type() == web_sys::Node::TEXT_NODE;
    }
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
        NodeKind::Image { .. } => tag == "img",
        NodeKind::Link { .. } => tag == "a",
        NodeKind::Media { .. } => tag == "audio" || tag == "video",
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
            // Same fix as `MountedRoot::unmount` — `container` is
            // `mounted.node`'s actual parent (it was `container.append_child`ed
            // directly), not `container`'s own parent.
            mounted.unmount();
            let _ = container.remove_child(&mounted.node);
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
    static CONTAINER_MOUNTS: std::cell::RefCell<HashMap<u64, MountedNode>> =
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

fn element_key(el: &Element) -> u64 {
    // Cheap, stable-per-element identity for the side-channel map. Keyed by the
    // full 64-bit pointer address (not truncated to `u32`) so it stays correct
    // under a memory64/wasm build, not just classic wasm32.
    (el.as_ref() as *const wasm_bindgen::JsValue as u64)
        ^ (el.tag_name().len() as u64).wrapping_mul(2654435761)
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
    let document = document()?;
    let node = render_node(&document, ui, &dispatch)?;
    let mounted = record_mount_tree(ui, &node);
    container.append_child(&node)?;
    Ok(MountedRoot {
        container: container.clone(),
        mounted,
        dispatch,
    })
}

/// Walks a freshly rendered DOM subtree alongside the `UITree` that produced
/// it and records child [`MountedNode`]s. Without this, the first
/// `MountedRoot::render` reconcile sees an empty child record list and
/// re-creates (appends) every child instead of diffing in place.
///
/// `render_node` appends exactly one DOM node per `Container`/`List` child in
/// order, so the k-th UI child maps to the k-th live child node. Deeper
/// subtree kinds (DataGrid's table internals, Checkbox's nested input) are
/// reconciled by direct DOM queries, so they need no records here.
fn record_mount_tree<Msg>(ui: &UITree<Msg>, node: &Node) -> MountedNode {
    let children: Vec<MountedNode> = match (&ui.kind, node.dyn_ref::<Element>()) {
        (
            NodeKind::Container {
                children: ui_children,
            },
            Some(el),
        ) => ui_children
            .iter()
            .enumerate()
            .filter_map(|(i, child_ui)| {
                el.child_nodes()
                    .get(i as u32)
                    .map(|child_node| record_mount_tree(child_ui, &child_node))
            })
            .collect(),
        // Non-windowed grids: one record per `tbody` row (rows are leaves —
        // cell updates go through `update_row_cells`, not child reconcile).
        (NodeKind::DataGrid { rows, .. }, Some(el)) if ui.meta.virtual_scroll.is_none() => el
            .query_selector("tbody")
            .ok()
            .flatten()
            .map(|tbody| {
                (0..rows.len())
                    .filter_map(|i| {
                        tbody.child_nodes().get(i as u32).map(|tr| MountedNode {
                            node: tr,
                            handles: Vec::new(),
                            children: Vec::new(),
                            key: None,
                            handlers: 0,
                            hash: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        // Non-windowed lists: one record per item node, matching `render_node` (items are bare
        // children of the `ul`). Windowed lists add spacers and are reconciled separately.
        (NodeKind::List { items }, Some(el)) if ui.meta.virtual_scroll.is_none() => items
            .iter()
            .enumerate()
            .filter_map(|(i, item_ui)| {
                el.child_nodes().get(i as u32).map(|item_node| {
                    let mut record = record_mount_tree(item_ui, &item_node);
                    record.key = Some(list_key(item_ui, i));
                    record
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    MountedNode {
        node: node.clone(),
        handles: Vec::new(),
        children,
        key: None,
        handlers: presence_mask(&ui.meta),
        hash: subtree_hash(ui),
    }
}

/// Like [`render`], but with `auto_optimize` enabled: large unconfigured
/// `List`/`DataGrid` nodes are windowed automatically on every reconcile, the
/// same auto-tune treatment `CanvasApp::auto_optimize` gives the canvas backend
/// and `TuiDriver::auto_optimize` gives the TUI backend.
pub fn render_with<Msg>(
    container: &Element,
    view: Rc<dyn Fn() -> UITree<Msg>>,
    dispatch: Rc<dyn Fn(Msg)>,
    auto_optimize: bool,
) -> Result<tpt_appfront_core::EffectHandle, wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let initial = view();
    let root = mount(container, &initial, Rc::clone(&dispatch))?;

    // `Element` is a refcounted handle to a JS object, so cloning it is cheap
    // and gives the effect an owned, `'static` value to read `client_height`
    // from on every reconcile.
    let container = container.clone();
    // `Option` so a kind-mismatch remount (below) can `.take()` the old root
    // to consume it via `unmount()`, which takes `self` by value.
    let root = std::cell::RefCell::new(Some(root));
    let handle = create_effect({
        let view = Rc::clone(&view);
        move || {
            let mut new_ui = view();
            if auto_optimize {
                let h = container.client_height() as f32;
                apply_auto_virtual_scroll(&mut new_ui, h);
            }
            let mut slot = root.borrow_mut();
            // `MountedRoot::render`'s own contract: `Err` means the new
            // tree's root kind is incompatible with the mounted DOM and the
            // caller "should unmount and re-mount". This used to be
            // `let _ = root.render(&new_ui)`, silently dropping that `Err`
            // — which meant *any* reactive state transition that changed a
            // node's kind anywhere in the tree (e.g. a `match` arm going
            // from rendering plain text to a `List`) left the DOM stuck on
            // stale content with no error, since `Err` from a kind mismatch
            // propagates up through every ancestor `reconcile_children`/
            // `reconcile_node` call via `?`, aborting the whole reconcile.
            let needs_remount = match slot.as_mut() {
                Some(r) => r.render(&new_ui).is_err(),
                None => true,
            };
            web_sys::console::log_1(
                &format!("DEBUG render_with: needs_remount={needs_remount}").into(),
            );
            if needs_remount {
                if let Some(old) = slot.take() {
                    old.unmount();
                }
                if let Ok(new_root) = mount(&container, &new_ui, Rc::clone(&dispatch)) {
                    *slot = Some(new_root);
                }
            }
        }
    });

    Ok(handle)
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
    render_with(container, view, dispatch, false)
}

/// Diffs `new_ui` against the live `mounted` record. A subtree whose fingerprint has not changed
/// since the last render is skipped: its DOM already shows it, so only its handler messages are
/// refreshed (see [`subtree_hash`]). Everything else goes through [`reconcile_node_inner`].
fn reconcile_node<Msg>(
    document: &Document,
    dispatch: &Rc<dyn Fn(Msg)>,
    mounted: &mut MountedNode,
    new_ui: &UITree<Msg>,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let hash = subtree_hash(new_ui);
    if let (Some(hash), Some(previous)) = (hash, mounted.hash) {
        if hash == previous && mounted.node.dyn_ref::<Element>().is_some() {
            rebind_subtree(mounted, new_ui, dispatch);
            return Ok(());
        }
    }
    reconcile_node_inner(document, dispatch, mounted, new_ui)?;
    mounted.hash = hash;
    Ok(())
}

/// A fingerprint of everything a re-render would write for `ui`, or `None` when the subtree must
/// always be reconciled. Handler messages are closures, so they are not part of the fingerprint.
/// Their presence is, because it decides which closures exist.
///
/// Not skippable: inputs, text areas, selects, checkboxes and radios (their value or checked state
/// lives in the DOM, and a re-render restores it), portals, data grids and windowed lists (no
/// one-to-one mounted records), and any subtree that contains one of these.
fn subtree_hash<Msg>(ui: &UITree<Msg>) -> Option<u64> {
    use std::hash::Hasher;
    if ui.meta.virtual_scroll.is_some() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_node(ui, &mut hasher)?;
    Some(hasher.finish())
}

fn hash_node<Msg>(ui: &UITree<Msg>, hasher: &mut impl std::hash::Hasher) -> Option<()> {
    use std::hash::Hash;
    ui.meta.key.hash(hasher);
    ui.meta.class.hash(hasher);
    ui.meta.attrs.hash(hasher);
    ui.meta.ai.action.hash(hasher);
    ui.meta.ai.params.hash(hasher);
    ui.meta.ai.description.hash(hasher);
    ui.meta.ai.title.hash(hasher);
    presence_mask(&ui.meta).hash(hasher);
    match &ui.kind {
        NodeKind::Container { children } => {
            0u8.hash(hasher);
            children.len().hash(hasher);
            for child in children {
                hash_node(child, hasher)?;
            }
        }
        NodeKind::List { items } => {
            1u8.hash(hasher);
            items.len().hash(hasher);
            for item in items {
                hash_node(item, hasher)?;
            }
        }
        NodeKind::Heading { level, text } => {
            2u8.hash(hasher);
            level.hash(hasher);
            text.hash(hasher);
        }
        NodeKind::Text { text } => {
            3u8.hash(hasher);
            text.hash(hasher);
        }
        NodeKind::Button { label } => {
            4u8.hash(hasher);
            label.hash(hasher);
        }
        NodeKind::Image { src, alt } => {
            5u8.hash(hasher);
            src.hash(hasher);
            alt.hash(hasher);
        }
        NodeKind::Link { href, text } => {
            6u8.hash(hasher);
            href.hash(hasher);
            text.hash(hasher);
        }
        _ => return None,
    }
    Some(())
}

/// Refreshes the handler messages of a subtree that was skipped. The subtree's fingerprint matched,
/// so its records and `ui` have the same shape.
fn rebind_subtree<Msg: Clone + 'static>(
    mounted: &MountedNode,
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
) {
    if presence_mask(&ui.meta) != 0 {
        rebind_handlers(&mounted.node, ui, dispatch);
    }
    match &ui.kind {
        NodeKind::Container { children } | NodeKind::List { items: children } => {
            for (record, child) in mounted.children.iter().zip(children) {
                rebind_subtree(record, child, dispatch);
            }
        }
        _ => {}
    }
}

/// Diffs `new_ui` against the live `mounted` record and updates the DOM *in
/// place*: reused nodes keep their identity (and listeners), only changed
/// attributes/text/children are touched, and removed subtrees are unmounted so
/// their listeners/handles are released. Returns `Err` if the root kind of
/// `new_ui` is incompatible with the mounted node (caller should full-replace).
fn reconcile_node_inner<Msg>(
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

            // Handlers are attached to the element, so a change in which handlers exist cannot be
            // applied in place. Handler messages that change are applied by rebinding below.
            let presence = presence_mask(&new_ui.meta);
            if presence != mounted.handlers {
                return Err(wasm_bindgen::JsValue::from_str(
                    "event handlers added or removed; full replace required",
                ));
            }
            if presence != 0 {
                rebind_handlers(&mounted.node, new_ui, dispatch);
            }

            match &new_ui.kind {
                NodeKind::Container {
                    children: new_children,
                } => {
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
                    if new_ui.meta.virtual_scroll.is_some() {
                        // Windowed lists have no mounted record per item, so the keyed diff cannot
                        // match them. They go to render_with's full unmount/re-mount path.
                        return Err(wasm_bindgen::JsValue::from_str(
                            "windowed list re-render requires a full replace",
                        ));
                    }
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
    // Keys come from the mounted records (`MountedNode::key`), not from `data-key` read back off
    // the DOM. Unkeyed children are matched by position.
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
        .map(|(i, m)| match key_fn {
            Some(_) => m.key.clone().unwrap_or_else(|| i.to_string()),
            None => i.to_string(),
        })
        .collect();

    // Debug builds report duplicate keys here. The result is not needed in release.
    #[cfg(debug_assertions)]
    let _: KeyedDiff<String> = reconcile_keys(&old_keys, &new_keys);

    // Fast path: the same keys in the same order. Every child is updated where it is, so
    // there is no map to build and nothing to move.
    if old_keys == new_keys {
        for (existing, new_child) in old_children.iter_mut().zip(new_children) {
            reconcile_node(document, dispatch, existing, new_child)?;
        }
        return Ok(());
    }

    // Pull mounted children into a reusable map keyed by their diff key.
    let mut old_by_key: HashMap<String, MountedNode> = HashMap::new();
    for (m, key) in old_children.drain(..).zip(old_keys) {
        old_by_key.insert(key, m);
    }

    let mut next: Vec<MountedNode> = Vec::with_capacity(new_children.len());
    for (i, new_child) in new_children.iter().enumerate() {
        let key = &new_keys[i];
        if let Some(mut existing) = old_by_key.remove(key) {
            reconcile_node(document, dispatch, &mut existing, new_child)?;
            position_child(parent, &existing.node, i)?;
            existing.key = Some(key.clone());
            next.push(existing);
        } else {
            let node = render_node(document, new_child, dispatch)?;
            // A keyed child must carry its key on the node, or the next diff cannot match it.
            if key_fn.is_some() {
                if let Some(el) = node.dyn_ref::<Element>() {
                    el.set_attribute("data-key", key)?;
                }
            }
            let mut mounted = record_mount_tree(new_child, &node);
            mounted.key = Some(key.clone());
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
            let _ = header_row.set_attribute("role", "row");
            for column in columns {
                let th = document.create_element("th")?;
                let _ = th.set_attribute("scope", "col");
                let _ = th.set_attribute("role", "columnheader");
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
        &(0..old_rows.len())
            .map(|i| i.to_string())
            .collect::<Vec<_>>(),
        &(0..new_rows.len())
            .map(|i| i.to_string())
            .collect::<Vec<_>>(),
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
                key: None,
                handlers: 0,
                hash: None,
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
    // ARIA grid row (todo.md #20).
    let _ = tr.set_attribute("role", "row");
    for cell in row {
        let td = document.create_element("td")?;
        let _ = td.set_attribute("role", "gridcell");
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

/// The browser window, or a `JsValue` error when there is none (for example in a non-browser
/// host). Used by the mount paths so a missing window is reported rather than panicking.
fn window() -> Result<web_sys::Window, wasm_bindgen::JsValue> {
    web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("no window"))
}

/// The window's document, or a `JsValue` error when either is missing.
fn document() -> Result<Document, wasm_bindgen::JsValue> {
    window()?
        .document()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("no document"))
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

/// Marker attribute listing (space-separated) the author-supplied attribute names last applied to
/// an element, so a patch can remove the ones the tree no longer carries.
const AUTHOR_ATTRS_MARKER: &str = "data-af-attrs";

/// Sets or removes a single attribute so the element matches `value`.
fn sync_attribute(el: &Element, name: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            if el.get_attribute(name).as_deref() != Some(value) {
                let _ = el.set_attribute(name, value);
            }
        }
        None => {
            if el.has_attribute(name) {
                let _ = el.remove_attribute(name);
            }
        }
    }
}

/// Applies author-supplied attributes (`title`, `aria-*`, …) and removes any that were applied
/// before but are no longer in `attrs`. Only names recorded in [`AUTHOR_ATTRS_MARKER`] are
/// removed, so attributes the browser or other code set are left alone.
fn sync_author_attrs(el: &Element, attrs: &[(String, String)]) {
    let previous: Vec<String> = el
        .get_attribute(AUTHOR_ATTRS_MARKER)
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    for name in &previous {
        if !attrs.iter().any(|(n, _)| n == name) {
            let _ = el.remove_attribute(name);
        }
    }
    for (name, value) in attrs {
        sync_attribute(el, name, Some(value));
    }
    if attrs.is_empty() {
        let _ = el.remove_attribute(AUTHOR_ATTRS_MARKER);
    } else {
        let names: Vec<&str> = attrs.iter().map(|(n, _)| n.as_str()).collect();
        sync_attribute(el, AUTHOR_ATTRS_MARKER, Some(&names.join(" ")));
    }
}

/// Applies the shared `NodeMeta` (class, ai action/params, author attributes) to a live element,
/// updating only what changed and removing what the tree no longer sets. `None` (text-node case)
/// is a no-op.
fn apply_meta_to_element<Msg>(el: Option<&Element>, ui: &UITree<Msg>) {
    let Some(el) = el else { return };
    sync_attribute(el, "class", ui.meta.class.as_deref());
    sync_attribute(el, "data-ai-action", ui.meta.ai.action.as_deref());
    let params_json = (!ui.meta.ai.params.is_empty() && ui.meta.ai.action.is_some())
        .then(|| serde_json::to_string(&json_obj(&ui.meta.ai.params)).unwrap_or_default());
    sync_attribute(el, "data-ai-params", params_json.as_deref());
    sync_author_attrs(el, &ui.meta.attrs);
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
                // Items are bare nodes that carry their key in `data-key`. The keyed diff in
                // `reconcile_children` reads that attribute and reuses the node, so a list
                // with a wrapper per item would not match what reconcile expects.
                for (i, item) in items.iter().enumerate() {
                    let item_node = render_node(document, item, dispatch)?;
                    if let Some(item_el) = item_node.dyn_ref::<Element>() {
                        item_el.set_attribute("data-key", &list_key(item, i))?;
                    }
                    el.append_child(&item_node)?;
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
            // Stable `id` so an author-supplied `<label for>` can associate with
            // this control (todo.md #20: label `for=`/`id` wiring).
            if let Some(id) = ui.meta.data_appfront_id {
                let _ = el.set_attribute("id", &format!("af-{id}"));
            }
            el.into()
        }
        NodeKind::Textarea { value } => {
            let el = document.create_element("textarea")?;
            if let Some(id) = ui.meta.data_appfront_id {
                let _ = el.set_attribute("id", &format!("af-{id}"));
            }
            el.set_text_content(Some(value));
            el.into()
        }
        NodeKind::Checkbox { label, checked } => {
            let el = document.create_element("label")?;
            // Explicit label association (`for`/`id`) in addition to the wrapping
            // `<label>`, so the control is named for assistive tech even if the
            // implicit containment is missed (todo.md #20).
            if let Some(id) = ui.meta.data_appfront_id {
                let _ = el.set_attribute("for", &format!("af-{id}"));
            }
            let input = document.create_element("input")?;
            input.set_attribute("type", "checkbox")?;
            if let Some(id) = ui.meta.data_appfront_id {
                let _ = input.set_attribute("id", &format!("af-{id}"));
            }
            if *checked {
                input.set_attribute("checked", "")?;
            }
            // Explicit ARIA state so screen readers announce the toggle (todo.md #16).
            input.set_attribute("aria-checked", if *checked { "true" } else { "false" })?;
            el.append_child(&input)?;
            let span = document.create_element("span")?;
            span.set_text_content(Some(label));
            el.append_child(&span)?;
            el.into()
        }
        NodeKind::Select { options, selected } => {
            let el = document.create_element("select")?;
            if let Some(id) = ui.meta.data_appfront_id {
                let _ = el.set_attribute("id", &format!("af-{id}"));
            }
            // A native `<select>` already exposes the correct implicit role; the
            // remaining gap is an accessible name when there's no associated
            // `<label>` (todo.md #20: Select ARIA).
            if let Some(desc) = &ui.meta.ai.description {
                if !desc.is_empty() {
                    let _ = el.set_attribute("aria-label", desc);
                }
            }
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
        NodeKind::Radio {
            name,
            options,
            selected,
        } => {
            let el = document.create_element("div")?;
            // `role="radiogroup"` groups the options for assistive tech.
            let _ = el.set_attribute("role", "radiogroup");
            for (i, (value, label)) in options.iter().enumerate() {
                let opt_id = ui
                    .meta
                    .data_appfront_id
                    .map(|g| format!("af-{g}-{i}"))
                    .unwrap_or_else(|| format!("af-opt-{i}"));
                let label_el = document.create_element("label")?;
                let _ = label_el.set_attribute("for", &opt_id);
                let input = document.create_element("input")?;
                input.set_attribute("type", "radio")?;
                input.set_attribute("name", name)?;
                input.set_attribute("value", value)?;
                let _ = input.set_attribute("id", &opt_id);
                if value == selected {
                    input.set_attribute("checked", "")?;
                }
                input.set_attribute(
                    "aria-checked",
                    if value == selected { "true" } else { "false" },
                )?;
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
            // `role="grid"` plus `role="row"`/`role="columnheader"`/`role="gridcell"`
            // make the table a proper ARIA grid (todo.md #20).
            let _ = table.set_attribute("role", "grid");

            let thead = document.create_element("thead")?;
            let header_row = document.create_element("tr")?;
            let _ = header_row.set_attribute("role", "row");
            for column in columns {
                let th = document.create_element("th")?;
                // `scope="col"` tells screen readers each header describes its column.
                let _ = th.set_attribute("scope", "col");
                let _ = th.set_attribute("role", "columnheader");
                th.set_text_content(Some(column));
                header_row.append_child(&th)?;
            }
            thead.append_child(&header_row)?;
            table.append_child(&thead)?;

            let tbody = document.create_element("tbody")?;
            if let Some(vs) = ui.meta.virtual_scroll {
                table.set_attribute(
                    "style",
                    &format!(
                        "display:block;overflow-y:auto;height:{}px",
                        vs.viewport_height
                    ),
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
        NodeKind::Image { src, alt } => {
            let el = document.create_element("img")?;
            el.set_attribute("src", src)?;
            el.set_attribute("alt", alt)?;
            el.into()
        }
        NodeKind::Link { href, text } => {
            let el = document.create_element("a")?;
            el.set_attribute("href", href)?;
            el.set_text_content(Some(text));
            el.into()
        }
        NodeKind::Media {
            src,
            alt,
            media_type,
        } => {
            let tag = match media_type {
                MediaType::Audio => "audio",
                MediaType::Video => "video",
            };
            let el = document.create_element(tag)?;
            el.set_attribute("src", src)?;
            el.set_attribute("controls", "")?;
            if !alt.is_empty() {
                el.set_attribute("aria-label", alt)?;
            }
            el.into()
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

    if let Some(el) = node.dyn_ref::<Element>() {
        sync_author_attrs(el, &ui.meta.attrs);
    }

    if let Some(action) = &ui.meta.ai.action {
        if let Some(el) = node.dyn_ref::<Element>() {
            el.set_attribute("data-ai-action", action)?;
            if !ui.meta.ai.params.is_empty() {
                let params_json =
                    serde_json::to_string(&json_obj(&ui.meta.ai.params)).unwrap_or_default();
                el.set_attribute("data-ai-params", &params_json)?;
            }
        }
    }

    attach_node_handlers(ui, dispatch, &node);
    Ok(node)
}

/// Handler bits for [`presence_mask`]: one per message handler a node can carry.
const H_CLICK: u32 = 1 << 0;
const H_INPUT: u32 = 1 << 1;
const H_TOGGLE: u32 = 1 << 2;
const H_FOCUS: u32 = 1 << 3;
const H_BLUR: u32 = 1 << 4;
const H_KEYDOWN: u32 = 1 << 5;
const H_POINTER_DOWN: u32 = 1 << 6;
const H_POINTER_MOVE: u32 = 1 << 7;
const H_POINTER_UP: u32 = 1 << 8;
const H_PASTE: u32 = 1 << 9;

/// Which message handlers `meta` declares, as one bit per handler. A re-render can update a
/// handler's message in place. It cannot add or remove one, so the bits must match.
fn presence_mask<Msg>(meta: &NodeMeta<Msg>) -> u32 {
    let mut mask = 0;
    let bits = [
        (meta.on_click.is_some(), H_CLICK),
        (meta.on_input.is_some(), H_INPUT),
        (meta.on_toggle.is_some(), H_TOGGLE),
        (meta.on_focus.is_some(), H_FOCUS),
        (meta.on_blur.is_some(), H_BLUR),
        (meta.on_keydown.is_some(), H_KEYDOWN),
        (meta.on_pointer_down.is_some(), H_POINTER_DOWN),
        (meta.on_pointer_move.is_some(), H_POINTER_MOVE),
        (meta.on_pointer_up.is_some(), H_POINTER_UP),
        (meta.on_paste.is_some(), H_PASTE),
    ];
    for (present, bit) in bits {
        if present {
            mask |= bit;
        }
    }
    mask
}

/// The message handlers an element has now, shared with the event closures attached to it.
///
/// Closures read these slots when an event fires instead of capturing a handler at attach time.
/// A re-render then only overwrites the slots. No closure is created and no DOM property is set.
struct HandlerSlots<Msg> {
    dispatch: Rc<dyn Fn(Msg)>,
    click: Option<Msg>,
    input: Option<OnInput<Msg>>,
    toggle: Option<OnToggle<Msg>>,
    focus: Option<Msg>,
    blur: Option<Msg>,
    keydown: Option<OnKeyDown<Msg>>,
    pointer_down: Option<OnPointerDown<Msg>>,
    pointer_move: Option<OnPointerMove<Msg>>,
    pointer_up: Option<OnPointerUp<Msg>>,
    paste: Option<OnPaste<Msg>>,
}

impl<Msg: Clone> HandlerSlots<Msg> {
    fn new(dispatch: &Rc<dyn Fn(Msg)>) -> Self {
        HandlerSlots {
            dispatch: Rc::clone(dispatch),
            click: None,
            input: None,
            toggle: None,
            focus: None,
            blur: None,
            keydown: None,
            pointer_down: None,
            pointer_move: None,
            pointer_up: None,
            paste: None,
        }
    }

    /// Overwrites every slot from `meta`. The presence check in `reconcile_node` guarantees the
    /// same handlers exist as before, so only their values change.
    fn update(&mut self, meta: &NodeMeta<Msg>, dispatch: &Rc<dyn Fn(Msg)>) {
        self.dispatch = Rc::clone(dispatch);
        self.click = meta.on_click.clone();
        self.input = meta.on_input.clone();
        self.toggle = meta.on_toggle.clone();
        self.focus = meta.on_focus.clone();
        self.blur = meta.on_blur.clone();
        self.keydown = meta.on_keydown.clone();
        self.pointer_down = meta.on_pointer_down.clone();
        self.pointer_move = meta.on_pointer_move.clone();
        self.pointer_up = meta.on_pointer_up.clone();
        self.paste = meta.on_paste.clone();
    }
}

type SharedSlots<Msg> = Rc<std::cell::RefCell<HandlerSlots<Msg>>>;

/// Sends `msg`, if there is one, through the slots' current dispatch. The dispatch is cloned out
/// first, so a handler that re-renders synchronously does not run into a live borrow.
fn fire<Msg: Clone>(slots: &std::cell::RefCell<HandlerSlots<Msg>>, msg: Option<Msg>) {
    if let Some(msg) = msg {
        let dispatch = Rc::clone(&slots.borrow().dispatch);
        dispatch(msg);
    }
}

/// Attaches every event handler `ui` declares to `node`. Mount runs it through [`render_node`]
/// and hydration through [`attach_listeners`]. A re-render updates the same slots through
/// [`rebind_handlers`].
fn attach_node_handlers<Msg: Clone + 'static>(
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
    node: &Node,
) {
    let presence = presence_mask(&ui.meta);
    if presence == 0 {
        return;
    }
    let mut initial = HandlerSlots::new(dispatch);
    initial.update(&ui.meta, dispatch);
    let slots: SharedSlots<Msg> = Rc::new(std::cell::RefCell::new(initial));
    // Holds the slots for as long as the node is tracked, so a re-render can find them.
    track_value(node, Box::new(Rc::clone(&slots)));
    attach_slot_closures(node, &slots, presence);
}

/// Creates one closure per present handler, each reading its message from `slots`.
fn attach_slot_closures<Msg: Clone + 'static>(
    node: &Node,
    slots: &SharedSlots<Msg>,
    presence: u32,
) {
    if let Some(el) = node.dyn_ref::<web_sys::HtmlElement>() {
        if presence & H_CLICK != 0 {
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut()>::new(move || {
                let msg = slots.borrow().click.clone();
                fire(&slots, msg);
            });
            el.set_onclick(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        if presence & H_FOCUS != 0 {
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut()>::new(move || {
                let msg = slots.borrow().focus.clone();
                fire(&slots, msg);
            });
            el.set_onfocus(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        if presence & H_BLUR != 0 {
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut()>::new(move || {
                let msg = slots.borrow().blur.clone();
                fire(&slots, msg);
            });
            el.set_onblur(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        if presence & H_KEYDOWN != 0 {
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |ev: web_sys::KeyboardEvent| {
                    let Some(on_keydown) = slots.borrow().keydown.clone() else {
                        return;
                    };
                    let event = KeyEvent {
                        key: ev.key(),
                        code: ev.code(),
                        ctrl: ev.ctrl_key(),
                        shift: ev.shift_key(),
                        alt: ev.alt_key(),
                        meta: ev.meta_key(),
                        repeat: ev.repeat(),
                    };
                    if let Some(msg) = on_keydown(event) {
                        ev.prevent_default();
                        fire(&slots, Some(msg));
                    }
                },
            );
            el.set_onkeydown(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        // A node with move or up handlers captures the pointer when it is pressed, so a drag keeps
        // reporting to it after the pointer leaves the element.
        if presence & (H_POINTER_DOWN | H_POINTER_MOVE | H_POINTER_UP) != 0 {
            let slots = Rc::clone(slots);
            let target = el.clone();
            let closure = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(
                move |ev: web_sys::PointerEvent| {
                    let (captures, on_down) = {
                        let s = slots.borrow();
                        (
                            s.pointer_move.is_some() || s.pointer_up.is_some(),
                            s.pointer_down.clone(),
                        )
                    };
                    if captures {
                        // Refused captures (the element is gone) only lose the drag, so ignore them.
                        let _ = target.set_pointer_capture(ev.pointer_id());
                    }
                    if let Some(on_down) = on_down {
                        let msg = on_down(pointer_event_from(&ev, &target));
                        fire(&slots, msg);
                    }
                },
            );
            el.set_onpointerdown(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        if presence & H_POINTER_MOVE != 0 {
            let slots = Rc::clone(slots);
            let target = el.clone();
            let closure = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(
                move |ev: web_sys::PointerEvent| {
                    let handler = slots.borrow().pointer_move.clone();
                    if let Some(handler) = handler {
                        let msg = handler(pointer_event_from(&ev, &target));
                        fire(&slots, msg);
                    }
                },
            );
            el.set_onpointermove(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        if presence & H_POINTER_UP != 0 {
            let slots = Rc::clone(slots);
            let target = el.clone();
            let closure = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(
                move |ev: web_sys::PointerEvent| {
                    let handler = slots.borrow().pointer_up.clone();
                    if let Some(handler) = handler {
                        let msg = handler(pointer_event_from(&ev, &target));
                        fire(&slots, msg);
                    }
                },
            );
            el.set_onpointerup(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }

        if presence & H_PASTE != 0 {
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::new(
                move |ev: web_sys::ClipboardEvent| {
                    let text = ev
                        .clipboard_data()
                        .and_then(|data| data.get_data("text/plain").ok())
                        .unwrap_or_default();
                    let handler = slots.borrow().paste.clone();
                    if let Some(handler) = handler {
                        if let Some(msg) = handler(text) {
                            ev.prevent_default();
                            fire(&slots, Some(msg));
                        }
                    }
                },
            );
            el.set_onpaste(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }
    }

    // Form controls: `oninput` on text fields and selects, `onchange` on a checkbox in a label.
    if presence & H_INPUT != 0 {
        if let Some(input_el) = node.dyn_ref::<web_sys::HtmlInputElement>() {
            let target = input_el.clone();
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut()>::new(move || {
                let msg = slots.borrow().input.as_ref().map(|f| f(target.value()));
                fire(&slots, msg);
            });
            input_el.set_oninput(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        } else if let Some(ta_el) = node.dyn_ref::<web_sys::HtmlTextAreaElement>() {
            let target = ta_el.clone();
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut()>::new(move || {
                let msg = slots.borrow().input.as_ref().map(|f| f(target.value()));
                fire(&slots, msg);
            });
            ta_el.set_oninput(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        } else if let Some(select_el) = node.dyn_ref::<web_sys::HtmlSelectElement>() {
            let target = select_el.clone();
            let slots = Rc::clone(slots);
            let closure = Closure::<dyn FnMut()>::new(move || {
                let msg = slots.borrow().input.as_ref().map(|f| f(target.value()));
                fire(&slots, msg);
            });
            select_el.set_onchange(Some(closure.as_ref().unchecked_ref()));
            track_closure(node, closure);
        }
    }

    if presence & H_TOGGLE != 0 {
        if let Some(label_el) = node.dyn_ref::<web_sys::HtmlLabelElement>() {
            if let Some(input_el) = label_el
                .query_selector("input[type=checkbox]")
                .ok()
                .flatten()
            {
                if let Some(checkbox_el) = input_el.dyn_ref::<web_sys::HtmlInputElement>() {
                    let target = checkbox_el.clone();
                    let slots = Rc::clone(slots);
                    let closure = Closure::<dyn FnMut()>::new(move || {
                        let msg = slots.borrow().toggle.as_ref().map(|f| f(target.checked()));
                        fire(&slots, msg);
                    });
                    checkbox_el.set_onchange(Some(closure.as_ref().unchecked_ref()));
                    track_closure(node, closure);
                }
            }
        }
    }
}

/// Finds the slots `node` was given at attach time and writes `ui`'s handler values into them.
/// Returns `false` when the node has none, so the caller attaches fresh closures instead.
fn update_handler_slots<Msg: Clone + 'static>(
    node: &Node,
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
) -> bool {
    let id = node_id(node);
    let found: Option<SharedSlots<Msg>> = LIVE_CLOSURES.with(|m| {
        m.borrow().get(&id).and_then(|entries| {
            entries.iter().find_map(|entry| {
                let any: &dyn std::any::Any = &**entry;
                any.downcast_ref::<SharedSlots<Msg>>().cloned()
            })
        })
    });
    match found {
        Some(slots) => {
            slots.borrow_mut().update(&ui.meta, dispatch);
            true
        }
        None => false,
    }
}

/// Brings `node`'s handlers up to date after a re-render whose handler set is unchanged (see
/// [`presence_mask`]). The message values are updated in place, so no closure is re-created.
fn rebind_handlers<Msg: Clone + 'static>(
    node: &Node,
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
) {
    if !update_handler_slots(node, ui, dispatch) {
        let previous = take_node_closures(node);
        attach_node_handlers(ui, dispatch, node);
        drop(previous);
    }
}

/// Attaches the handlers and AI attributes of `ui` to an element that already exists in the DOM,
/// as hydration does for server-rendered markup.
fn attach_listeners<Msg>(
    ui: &UITree<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
    el: &Element,
) -> Result<(), wasm_bindgen::JsValue>
where
    Msg: Clone + 'static,
{
    let node: &Node = el.as_ref();
    attach_node_handlers(ui, dispatch, node);

    if let Some(action) = &ui.meta.ai.action {
        el.set_attribute("data-ai-action", action)?;
        if !ui.meta.ai.params.is_empty() {
            let params_json =
                serde_json::to_string(&json_obj(&ui.meta.ai.params)).unwrap_or_default();
            el.set_attribute("data-ai-params", &params_json)?;
        }
    }

    Ok(())
}

// Keeps event `Closure`s alive for as long as their DOM node lives, keyed by
// a stable per-node id so a re-render that replaces the handler can find and
// drop the old one. `unmount` clears the entry when it clears the handler.
// Keyed by `u64`, not `u32`: the node pointer is widened before hashing so the
// mapping stays correct if wasm64/`memory64` ever lands (a `u32` truncation of
// a pointer would silently collide identities and drop the wrong closures).
//
// The closures have different signatures (no argument, keyboard, pointer, clipboard), so each is
// stored type-erased. Dropping the `Box` drops the `Closure`.
type LiveClosures = std::cell::RefCell<HashMap<u64, Vec<Box<dyn std::any::Any>>>>;
thread_local! {
    static LIVE_CLOSURES: LiveClosures = std::cell::RefCell::new(HashMap::new());
}

/// Stable identity for a DOM node across renders, used to map its closures for cleanup.
///
/// This is a number stored on the JavaScript object, not the address of the Rust `Node`. A `Node`
/// is moved when it is returned from `render_node` and when it is stored in a `MountedNode`, so an
/// address-based key would differ between attaching a handler and releasing it.
fn node_id(node: &Node) -> u64 {
    thread_local! {
        static NEXT_UID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    let key = wasm_bindgen::JsValue::from_str("__appfront_uid");
    let target: &wasm_bindgen::JsValue = node.as_ref();
    if let Ok(existing) = js_sys::Reflect::get(target, &key) {
        if let Some(uid) = existing.as_f64() {
            return uid as u64;
        }
    }
    let uid = NEXT_UID.with(|next| {
        let uid = next.get() + 1;
        next.set(uid);
        uid
    });
    let _ = js_sys::Reflect::set(target, &key, &wasm_bindgen::JsValue::from_f64(uid as f64));
    uid
}

/// Removes and returns every closure tracked for `node`, without dropping them.
fn take_node_closures(node: &Node) -> Vec<Box<dyn std::any::Any>> {
    let id = node_id(node);
    LIVE_CLOSURES.with(|m| m.borrow_mut().remove(&id).unwrap_or_default())
}

/// The appfront pointer event for a DOM pointer event dispatched to `target`, the element that
/// has the handler.
fn pointer_event_from(ev: &web_sys::PointerEvent, target: &web_sys::HtmlElement) -> PointerEvent {
    let rect = target.get_bounding_client_rect();
    PointerEvent {
        x: f64::from(ev.client_x()),
        y: f64::from(ev.client_y()),
        button: ev.button(),
        ctrl: ev.ctrl_key(),
        shift: ev.shift_key(),
        alt: ev.alt_key(),
        meta: ev.meta_key(),
        target_width: rect.width(),
        target_height: rect.height(),
    }
}

fn track_closure<T: ?Sized + 'static>(node: &Node, closure: Closure<T>) {
    track_value(node, Box::new(closure));
}

/// Keeps `value` (a closure, or the slots its closures read) alive until `node` is released.
fn track_value(node: &Node, value: Box<dyn std::any::Any>) {
    let id = node_id(node);
    LIVE_CLOSURES.with(|m| m.borrow_mut().entry(id).or_default().push(value));
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

    // No window (e.g. a non-browser host) means there is nothing to flush into.
    let Some(window) = web_sys::window() else {
        TEXT_FLUSH_SCHEDULED.with(|s| s.set(false));
        return;
    };
    let closure = Closure::once(flush_text_updates);
    // `Closure::once` is consumed by the JS callback invocation itself; the
    // `.forget()` here only leaks if the browser never fires the callback
    // (e.g. the page is torn down first), the same tradeoff every other
    // fire-and-forget closure in this module makes.
    if window
        .request_animation_frame(closure.as_ref().unchecked_ref())
        .is_err()
    {
        // Could not schedule: clear the flag so a later update retries, and drop the closure.
        TEXT_FLUSH_SCHEDULED.with(|s| s.set(false));
        return;
    }
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
    let document = document()?;

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
        // A portal renders its content in place, so interactive content inside it needs hydrating
        // just like any other child.
        NodeKind::Portal { content, .. } => {
            if hydrate_node(content, dispatch, id_map)? {
                needs_hydration = true;
            }
        }
        // Image, Link and Media have no child nodes; their own handlers are covered by the
        // `needs_hydration` check above.
        NodeKind::Image { .. } | NodeKind::Link { .. } | NodeKind::Media { .. } => {}
        NodeKind::DataGrid { .. }
        | NodeKind::Heading { .. }
        | NodeKind::Text { .. }
        | NodeKind::Button { .. }
        | NodeKind::Input { .. }
        | NodeKind::Textarea { .. }
        | NodeKind::Checkbox { .. }
        | NodeKind::Select { .. }
        | NodeKind::Radio { .. } => {}
    }

    if needs_hydration {
        if let Some(id) = ui.meta.data_appfront_id {
            if let Some(el) = id_map.get(&id) {
                attach_listeners(ui, dispatch, el)?;
            } else {
                // SSR/CSR mismatch: an interactive node was built on the client
                // but the server's serialized DOM had no matching
                // `data-appfront-id`. In debug builds surface it so the broken
                // hydration is obvious rather than silently leaving a dead node.
                #[cfg(debug_assertions)]
                web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(&format!(
                    "tpt-appfront hydrate: no DOM node for appfront id {id}; \
                         SSR/CSR mismatch — listeners not attached"
                )));
            }
        } else {
            // An interactive node with no id at all also indicates a server
            // render that didn't emit `data-appfront-id` on it.
            #[cfg(debug_assertions)]
            web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(
                "tpt-appfront hydrate: interactive node missing data-appfront-id; \
                 SSR/CSR mismatch — listeners not attached",
            ));
        }
    }

    Ok(needs_hydration)
}
