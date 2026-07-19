//! The abstract `UITree` AST (see `spec.txt` section 3.1).
//!
//! Every node carries a type-specific [`NodeKind`] plus shared [`NodeMeta`]
//! (styling class, event bindings, AI metadata). `Msg` is the application's
//! own event enum — e.g. `on_click(Event::ExportData)` in the spec's
//! example — so the core crate never needs to know what events an app defines.

use serde::{Deserialize, Serialize};

use crate::virtual_scroll::VirtualScroll;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UITree<Msg> {
    pub kind: NodeKind<Msg>,
    pub meta: NodeMeta<Msg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind<Msg> {
    Container { children: Vec<UITree<Msg>> },
    Heading { level: u8, text: String },
    Text { text: String },
    Button { label: String },
    Input { value: String },
    /// Multi-line text input.
    Textarea { value: String },
    /// A single boolean toggle, e.g. `<input type="checkbox">`. Two-way bound
    /// via [`NodeMeta::on_toggle`] rather than [`NodeMeta::on_input`] since its
    /// value is a `bool`, not a `String`.
    Checkbox { label: String, checked: bool },
    /// A single-choice dropdown. `options` is `(value, label)` pairs;
    /// `selected` is the currently-chosen option's `value`. Two-way bound via
    /// [`NodeMeta::on_input`] (the new selected value).
    Select {
        options: Vec<(String, String)>,
        selected: String,
    },
    /// A single-choice radio button group. `name` groups the individual radio
    /// inputs so selecting one clears the others — required by HTML's radio
    /// semantics and mirrored by non-DOM backends for consistency. `options`
    /// is `(value, label)` pairs; `selected` is the currently-chosen value.
    /// Two-way bound via [`NodeMeta::on_input`].
    Radio {
        name: String,
        options: Vec<(String, String)>,
        selected: String,
    },
    List { items: Vec<UITree<Msg>> },
    DataGrid {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// A "portal": its `content` is rendered not inline at this node's position
    /// but into the named portal *target* instead. Hosts (DOM/canvas/TUI)
    /// render portal targets as overlay layers (modal/toast/tooltip surfaces)
    /// regardless of where in the logical tree the portal was declared. See
    /// [`UITree::collect_portals`] and [`ContainerBuilder::portal`]. Backend-
    /// agnostic: a backend that doesn't support portal targets can simply
    /// inline `content` at the declaration site as a fallback.
    Portal {
        target: String,
        content: Box<UITree<Msg>>,
    },
}

/// AI-agent metadata attached to any node (see `docs/ai-schema.md`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiMeta {
    /// Machine-readable action name (e.g. `"add_to_cart"`). When set, the
    /// node is considered an interactive action that an AI agent can invoke.
    pub action: Option<String>,
    /// Key-value parameter map the action expects.
    pub params: Vec<(String, String)>,
    /// Human-readable description of what this element does.
    pub description: Option<String>,
}

/// Two-way-binding callback for string-valued form nodes (`Input`,
/// `Textarea`, `Select`, `Radio`): takes the control's new string value
/// (known only once the change event fires) and produces a `Msg` to
/// dispatch, mirroring `on_click`'s dispatch pattern but parameterized by a
/// runtime value instead of a value baked in at tree-build time. `Arc<dyn Fn
/// + Send + Sync>` (not `Rc`) — same reasoning as
/// `tpt_appfront_server::router::CommandHandler`: a `UITree` can end up behind
/// an `Arc<SmartRouter<Msg>>` shared across an Axum server's worker threads,
/// which requires every field to be `Send + Sync`.
pub type OnInput<Msg> = std::sync::Arc<dyn Fn(String) -> Msg + Send + Sync>;

/// Two-way-binding callback for `Checkbox` nodes: takes the checkbox's new
/// `checked` state and produces a `Msg` to dispatch. Separate from
/// [`OnInput`] since a checkbox's value is a `bool`, not a `String`.
pub type OnToggle<Msg> = std::sync::Arc<dyn Fn(bool) -> Msg + Send + Sync>;

/// `#[serde(default = "...")]` target for [`NodeMeta::on_input`]. Needed
/// because plain `#[serde(skip)]` makes serde's derive require `Msg:
/// Default` (it infers the bound from the field's generic parameters, not
/// realizing `Option<T>: Default` doesn't actually need `T: Default`) —
/// spelling out the default function sidesteps that overly-strict inference.
fn on_input_default<Msg>() -> Option<OnInput<Msg>> {
    None
}

/// `#[serde(default = "...")]` target for [`NodeMeta::on_toggle`] — see
/// [`on_input_default`] for why this can't just be `#[serde(skip)]`.
fn on_toggle_default<Msg>() -> Option<OnToggle<Msg>> {
    None
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NodeMeta<Msg> {
    pub class: Option<String>,
    pub on_click: Option<Msg>,
    /// See [`OnInput`]. Not serializable — a live closure can't survive
    /// SSR/hydration JSON, and SSR/AI-schema rendering never needs to *call*
    /// it, only know an input exists. Currently only `tpt-appfront-dom` wires
    /// this into a real `oninput` listener; `tpt-appfront-canvas`/`tpt-appfront-tui`
    /// don't consume it yet.
    #[serde(skip, default = "on_input_default")]
    pub on_input: Option<OnInput<Msg>>,
    /// See [`OnToggle`]. Two-way binding for `Checkbox` nodes; not
    /// serializable, same reasoning as `on_input`.
    #[serde(skip, default = "on_toggle_default")]
    pub on_toggle: Option<OnToggle<Msg>>,
    pub ai: AiMeta,
    /// Stable identifier assigned before SSR so the client hydrator can match
    /// server-rendered DOM nodes back to their `UITree` counterpart.
    pub data_appfront_id: Option<u64>,
    /// Whether the subtree this node roots was produced by a
    /// `#[tpt_appfront_core::component]` function whose body reads any
    /// `Signal`. `false` (the default) means either the node wasn't
    /// produced by the macro, or the macro's static analysis found no
    /// signal reads in the function body. Backends can use this as a hint
    /// to skip hydration/listener work for subtrees that never change
    /// (see Phase 9 islands hydration).
    #[serde(default)]
    pub is_dynamic: bool,
    /// Arbitrary key/value attributes rendered verbatim by backends. Used for
    /// accessibility (`role`, `aria-*`, `tabindex`), semantic landmarks
    /// (`aria-label`, `role="navigation"`), and any backend-specific attribute
    /// that has no first-class `NodeMeta` field. Serialized as-is, so values
    /// must be `String` (no closures). Backends render these as DOM attributes
    /// (HTML/SSR), and ignore the ones they don't model (canvas/TUI).
    #[serde(default)]
    pub attrs: Vec<(String, String)>,
    /// Stable identity for reconciliation, e.g. a row/entity id. Set via
    /// [`NodeRef::key`] on items inside a [`NodeKind::List`]/[`NodeKind::DataGrid`]
    /// so backends can diff add/remove/reorder against a previous render
    /// instead of rebuilding every child from scratch.
    #[serde(default)]
    pub key: Option<String>,
    /// Windowed-rendering config for `List`/`DataGrid` nodes — see
    /// [`VirtualScroll`]. `None` (the default) means render every item.
    #[serde(default)]
    pub virtual_scroll: Option<VirtualScroll>,
}

impl<Msg> Default for NodeMeta<Msg> {
    fn default() -> Self {
        NodeMeta {
            class: None,
            on_click: None,
            on_input: None,
            on_toggle: None,
            ai: AiMeta::default(),
            data_appfront_id: None,
            is_dynamic: false,
            attrs: Vec::new(),
            key: None,
            virtual_scroll: None,
        }
    }
}

/// Manual impl since `on_input`'s `Rc<dyn Fn(String) -> Msg>` can't derive
/// `Debug` (trait objects for `Fn` don't implement it) — every other field
/// still prints normally, `on_input` prints as a presence marker.
impl<Msg: std::fmt::Debug> std::fmt::Debug for NodeMeta<Msg> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeMeta")
            .field("class", &self.class)
            .field("on_click", &self.on_click)
            .field("on_input", &self.on_input.as_ref().map(|_| "<fn>"))
            .field("on_toggle", &self.on_toggle.as_ref().map(|_| "<fn>"))
            .field("ai", &self.ai)
            .field("data_appfront_id", &self.data_appfront_id)
            .field("is_dynamic", &self.is_dynamic)
            .field("key", &self.key)
            .field("virtual_scroll", &self.virtual_scroll)
            .finish()
    }
}

impl<Msg> UITree<Msg> {
    fn leaf(kind: NodeKind<Msg>) -> Self {
        UITree {
            kind,
            meta: NodeMeta::default(),
        }
    }

    /// Builds a `Container` node from a closure, mirroring the spec's
    /// `UITree::container(|c| { ... })` ergonomics.
    pub fn container(build: impl FnOnce(&mut ContainerBuilder<Msg>)) -> Self {
        let mut builder = ContainerBuilder { children: Vec::new() };
        build(&mut builder);
        UITree::leaf(NodeKind::Container {
            children: builder.children,
        })
    }

    pub fn meta_mut(&mut self) -> &mut NodeMeta<Msg> {
        &mut self.meta
    }

    /// Collects the contents of every portal targeting `target` anywhere in
    /// this tree (including nested portals), in document order. Hosts render
    /// these as an overlay layer independent of where each portal was declared.
    /// Portals that target a *different* name are ignored (but still walked, so
    /// nested portals are found wherever they live).
    pub fn collect_portals(&self, target: &str) -> Vec<UITree<Msg>>
    where
        Msg: Clone,
    {
        fn walk<Msg: Clone>(ui: &UITree<Msg>, target: &str, out: &mut Vec<UITree<Msg>>) {
            match &ui.kind {
                NodeKind::Container { children } => {
                    for child in children {
                        walk(child, target, out);
                    }
                }
                NodeKind::List { items } => {
                    for item in items {
                        walk(item, target, out);
                    }
                }
                NodeKind::Portal {
                    target: t,
                    content,
                } => {
                    if t == target {
                        out.push((**content).clone());
                    } else {
                        // Still recurse: a nested portal may target `target`.
                        walk(content, target, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(self, target, &mut out);
        out
    }

    /// Names of all distinct portal targets referenced anywhere in this tree.
    /// Useful for a host to pre-create its overlay layers.
    pub fn portal_targets(&self) -> std::collections::BTreeSet<String> {
        fn walk<Msg>(ui: &UITree<Msg>, out: &mut std::collections::BTreeSet<String>) {
            match &ui.kind {
                NodeKind::Container { children } => {
                    for child in children {
                        walk(child, out);
                    }
                }
                NodeKind::List { items } => {
                    for item in items {
                        walk(item, out);
                    }
                }
                NodeKind::Portal { target, content } => {
                    out.insert(target.clone());
                    walk(content, out);
                }
                _ => {}
            }
        }
        let mut out = std::collections::BTreeSet::new();
        walk(self, &mut out);
        out
    }

    /// Walks the tree and assigns a unique sequential [`NodeMeta::data_appfront_id`]
    /// to every node. Safe to call multiple times — previously assigned IDs are
    /// overwritten.
    pub fn assign_ids(&mut self) {
        fn walk<Msg>(ui: &mut UITree<Msg>, next: &mut u64) {
            ui.meta.data_appfront_id = Some(*next);
            *next += 1;
            match &mut ui.kind {
                NodeKind::Container { children } => {
                    for child in children {
                        walk(child, next);
                    }
                }
                NodeKind::List { items } => {
                    for item in items {
                        walk(item, next);
                    }
                }
                NodeKind::Portal { content, .. } => {
                    walk(content, next);
                }
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
        }
        walk(self, &mut 1);
    }
}

/// Payload serialised into `<script id="__APPFRONT_STATE__">` during SSR and
/// consumed by [`hydrate`][crate::dom::hydrate] on the client to resume
/// interactivity without re-creating DOM nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrationPayload<Msg> {
    /// The full tree (with `data_appfront_id` filled).
    pub tree: UITree<Msg>,
    /// Named signal values that the client should restore before effects fire.
    pub signals: std::collections::HashMap<String, serde_json::Value>,
}

/// Passed into the closure given to [`UITree::container`]; each method
/// appends a child node and returns a [`NodeRef`] so callers can chain
/// `.class(...)` / `.on_click(...)` onto the node they just added.
pub struct ContainerBuilder<Msg> {
    children: Vec<UITree<Msg>>,
}

impl<Msg> ContainerBuilder<Msg> {
    /// Creates an empty builder. Used by macro codegen for static-subtree
    /// caching (the `view!`/`#[component]` `static_tree` path), which builds
    /// a one-off subtree and extracts it via [`ContainerBuilder::into_only_child`].
    pub fn new() -> Self {
        ContainerBuilder {
            children: Vec::new(),
        }
    }

    /// Consumes the builder and returns its single child (the result of a
    /// macro-generated subtree built via [`ContainerBuilder::new`]). Panics if
    /// the builder produced zero or more than one child, since the static-
    /// subtree codegen always builds exactly one root node.
    pub fn into_only_child(self) -> Option<UITree<Msg>> {
        if self.children.len() == 1 {
            Some(self.children.into_iter().next().unwrap())
        } else {
            None
        }
    }

    fn push(&mut self, kind: NodeKind<Msg>) -> NodeRef<'_, Msg> {
        self.children.push(UITree::leaf(kind));
        let index = self.children.len() - 1;
        NodeRef {
            children: &mut self.children,
            index,
        }
    }

    pub fn container(&mut self, build: impl FnOnce(&mut ContainerBuilder<Msg>)) -> NodeRef<'_, Msg> {
        let node = UITree::container(build);
        self.children.push(node);
        let index = self.children.len() - 1;
        NodeRef {
            children: &mut self.children,
            index,
        }
    }

    /// Appends an already-built `UITree<Msg>` as a child and returns a
    /// [`NodeRef`] to it. Primarily used by the `view!` macro's static-subtree
    /// codegen, which hands a [`crate::static_tree::static_node`] instance
    /// here instead of rebuilding the subtree inline every render.
    pub fn with(&mut self, node: UITree<Msg>) -> NodeRef<'_, Msg> {
        self.children.push(node);
        let index = self.children.len() - 1;
        NodeRef {
            children: &mut self.children,
            index,
        }
    }

    pub fn heading(&mut self, level: u8, text: impl Into<String>) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Heading {
            level,
            text: text.into(),
        })
    }

    pub fn text(&mut self, text: impl Into<String>) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Text { text: text.into() })
    }

    pub fn button(&mut self, label: impl Into<String>) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Button {
            label: label.into(),
        })
    }

    pub fn input(&mut self, value: impl Into<String>) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Input {
            value: value.into(),
        })
    }

    /// A multi-line text input. Two-way bound via [`NodeRef::on_input`].
    pub fn textarea(&mut self, value: impl Into<String>) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Textarea {
            value: value.into(),
        })
    }

    /// A boolean toggle. Two-way bound via [`NodeRef::on_toggle`].
    pub fn checkbox(&mut self, label: impl Into<String>, checked: bool) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Checkbox {
            label: label.into(),
            checked,
        })
    }

    /// A single-choice dropdown. `options` is `(value, label)` pairs.
    /// Two-way bound via [`NodeRef::on_input`] (the newly selected value).
    pub fn select(
        &mut self,
        options: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
        selected: impl Into<String>,
    ) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Select {
            options: options
                .into_iter()
                .map(|(v, l)| (v.into(), l.into()))
                .collect(),
            selected: selected.into(),
        })
    }

    /// A single-choice radio button group sharing `name`. `options` is
    /// `(value, label)` pairs. Two-way bound via [`NodeRef::on_input`] (the
    /// newly selected value).
    pub fn radio_group(
        &mut self,
        name: impl Into<String>,
        options: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
        selected: impl Into<String>,
    ) -> NodeRef<'_, Msg> {
        self.push(NodeKind::Radio {
            name: name.into(),
            options: options
                .into_iter()
                .map(|(v, l)| (v.into(), l.into()))
                .collect(),
            selected: selected.into(),
        })
    }

    pub fn list(&mut self, build: impl FnOnce(&mut ContainerBuilder<Msg>)) -> NodeRef<'_, Msg> {
        let mut inner = ContainerBuilder { children: Vec::new() };
        build(&mut inner);
        self.push(NodeKind::List {
            items: inner.children,
        })
    }

    /// Declares a portal: `build` produces a subtree that is rendered into the
    /// named portal *target* (an overlay layer) rather than inline. The host
    /// collects portals via [`UITree::collect_portals`] and renders each target
    /// independently — this is how modals/toasts/tooltips escape their logical
    /// parent's clipping/stacking context. Returns a [`NodeRef`] to the portal
    /// node (the declaration site) for chaining `class`/`key` onto it.
    pub fn portal(
        &mut self,
        target: impl Into<String>,
        build: impl FnOnce(&mut ContainerBuilder<Msg>),
    ) -> NodeRef<'_, Msg> {
        let mut inner = ContainerBuilder { children: Vec::new() };
        build(&mut inner);
        let single = inner.into_only_child().unwrap_or_else(|| {
            UITree::container(|_| {})
        });
        self.push(NodeKind::Portal {
            target: target.into(),
            content: Box::new(single),
        })
    }

    pub fn data_grid(
        &mut self,
        columns: impl IntoIterator<Item = impl Into<String>>,
        rows: impl IntoIterator<Item = impl IntoIterator<Item = impl Into<String>>>,
    ) -> NodeRef<'_, Msg> {
        self.push(NodeKind::DataGrid {
            columns: columns.into_iter().map(Into::into).collect(),
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().map(Into::into).collect())
                .collect(),
        })
    }
}

impl<Msg> Default for ContainerBuilder<Msg> {
    fn default() -> Self {
        Self::new()
    }
}

/// A chainable reference to the node most recently pushed onto a
/// [`ContainerBuilder`], used to set styling/events without needing a
/// separate variable per node.
pub struct NodeRef<'a, Msg> {
    children: &'a mut Vec<UITree<Msg>>,
    index: usize,
}

impl<'a, Msg> NodeRef<'a, Msg> {
    fn meta_mut(&mut self) -> &mut NodeMeta<Msg> {
        self.children[self.index].meta_mut()
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.meta_mut().class = Some(class.into());
        self
    }

    pub fn on_click(mut self, msg: Msg) -> Self {
        self.meta_mut().on_click = Some(msg);
        self
    }

    /// Two-way binding for `Input` nodes: `f` is called with the input's new
    /// value on every change, producing a `Msg` to dispatch. See
    /// [`OnInput`]. Currently only wired up by `tpt-appfront-dom`.
    pub fn on_input(mut self, f: impl Fn(String) -> Msg + Send + Sync + 'static) -> Self {
        self.meta_mut().on_input = Some(std::sync::Arc::new(f));
        self
    }

    /// Two-way binding for `Checkbox` nodes: `f` is called with the
    /// checkbox's new `checked` state on every change. See [`OnToggle`].
    pub fn on_toggle(mut self, f: impl Fn(bool) -> Msg + Send + Sync + 'static) -> Self {
        self.meta_mut().on_toggle = Some(std::sync::Arc::new(f));
        self
    }

    pub fn ai_action(mut self, action: impl Into<String>) -> Self {
        self.meta_mut().ai.action = Some(action.into());
        self
    }

    pub fn ai_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta_mut().ai.params.push((key.into(), value.into()));
        self
    }

    pub fn ai_description(mut self, desc: impl Into<String>) -> Self {
        self.meta_mut().ai.description = Some(desc.into());
        self
    }

    /// Sets an arbitrary attribute (e.g. `role`, `tabindex`, `aria-*`,
    /// `placeholder`). Rendered verbatim by backends that model HTML
    /// attributes; ignored where unsupported. See [`NodeMeta::attrs`].
    pub fn attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta_mut().attrs.push((name.into(), value.into()));
        self
    }

    /// Convenience for ARIA attributes (e.g. `self.aria("label", "menu")`
    /// emits `aria-label="menu"`).
    pub fn aria(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let mut full = String::from("aria-");
        full.push_str(&name.into());
        self.attr(full, value)
    }

    /// Stable identity for reconciliation (e.g. a row/entity id) — see
    /// [`NodeMeta::key`].
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.meta_mut().key = Some(key.into());
        self
    }

    /// Enables windowed rendering for a `List`/`DataGrid` — see
    /// [`VirtualScroll`].
    pub fn virtual_scroll(mut self, config: VirtualScroll) -> Self {
        self.meta_mut().virtual_scroll = Some(config);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum Event {
        ExportData,
    }

    fn sample_ui() -> UITree<Event> {
        UITree::container(|c| {
            c.heading(1, "Dashboard").class("text-2xl font-bold");
            c.data_grid(["Name", "Value"], [vec!["a", "1"], vec!["b", "2"]])
                .class("w-full mt-4");
            c.button("Export").on_click(Event::ExportData);
        })
    }

    #[test]
    fn builder_produces_expected_shape() {
        let ui = sample_ui();
        let NodeKind::Container { children } = ui.kind else {
            panic!("expected container");
        };
        assert_eq!(children.len(), 3);

        match &children[0].kind {
            NodeKind::Heading { level, text } => {
                assert_eq!(*level, 1);
                assert_eq!(text, "Dashboard");
            }
            _ => panic!("expected heading"),
        }
        assert_eq!(
            children[0].meta.class.as_deref(),
            Some("text-2xl font-bold")
        );

        match &children[1].kind {
            NodeKind::DataGrid { columns, rows } => {
                assert_eq!(columns, &["Name", "Value"]);
                assert_eq!(rows.len(), 2);
            }
            _ => panic!("expected data grid"),
        }

        match &children[2].kind {
            NodeKind::Button { label } => assert_eq!(label, "Export"),
            _ => panic!("expected button"),
        }
        assert_eq!(children[2].meta.on_click, Some(Event::ExportData));
    }

    #[test]
    fn round_trips_through_json() {
        let ui = sample_ui();
        let json = serde_json::to_string(&ui).expect("serialize");
        let restored: UITree<Event> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            format!("{restored:?}"),
            format!("{:?}", ui),
            "round-tripped tree should match the original"
        );
    }

    #[test]
    fn assign_ids_assigns_sequential_ids() {
        let mut ui = UITree::container(|c| {
            c.heading(2, "Section");
            c.list(|l| {
                l.text("item");
            });
            c.container(|inner| {
                inner.button("Go").on_click(Event::ExportData);
            });
        });

        ui.assign_ids();

        // Container root = 1
        assert_eq!(ui.meta.data_appfront_id, Some(1));

        let NodeKind::Container { children } = &ui.kind else {
            panic!("expected container");
        };

        // heading = 2, list = 3, nested container = 5
        assert_eq!(children[0].meta.data_appfront_id, Some(2));
        assert_eq!(children[1].meta.data_appfront_id, Some(3));

        let NodeKind::List { items } = &children[1].kind else {
            panic!("expected list");
        };
        assert_eq!(items[0].meta.data_appfront_id, Some(4));

        assert_eq!(children[2].meta.data_appfront_id, Some(5));

        let NodeKind::Container { children: inner_children } = &children[2].kind else {
            panic!("expected container");
        };
        assert_eq!(inner_children[0].meta.data_appfront_id, Some(6));
    }

    #[test]
    fn hydration_payload_round_trips() {
        let mut ui = sample_ui();
        ui.assign_ids();

        let mut signals = std::collections::HashMap::new();
        signals.insert("count".to_string(), serde_json::json!(42));

        let payload = HydrationPayload {
            tree: ui,
            signals: signals.clone(),
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: HydrationPayload<Event> =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.tree.meta.data_appfront_id, Some(1));
        assert_eq!(restored.signals.get("count"), signals.get("count"));
    }
}
