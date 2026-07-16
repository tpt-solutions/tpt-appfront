//! Programmatic API for AI agents to inspect and interact with the UI tree
//! without needing a browser or canvas.
//!
//! # Functions
//!
//! | Function | Purpose |
//! |---|---|
//! | [`query_state`] | Walk the tree and return a structured [`AgentState`] snapshot |
//! | [`navigate_to`]  | Change the current route (signals reactive system) |
//! | [`trigger_event`] | Dispatch an event by `ai_action` name |
//! | [`current_route`] | Return the current route path |

use serde::Serialize;
use std::cell::RefCell;

use crate::signal::Signal;
use crate::ui_tree::{NodeKind, UITree};

/// Structured snapshot of the UI tree designed for LLM / AI agent consumption.
///
/// Returned by [`query_state`] and serializable as JSON.
#[derive(Debug, Clone, Serialize)]
pub struct AgentState {
    /// Elements an agent can interact with (buttons, inputs, ...).
    pub interactive_elements: Vec<ElementSummary>,
    /// Elements that display data (headings, text, grids, ...).
    pub data_elements: Vec<ElementSummary>,
    /// The current route known to the reactive router.
    pub current_route: String,
}

/// Lightweight description of a single UI element.
#[derive(Debug, Clone, Serialize)]
pub struct ElementSummary {
    /// Stable element id assigned by [`UITree::assign_ids`] (if called).
    pub id: Option<u64>,
    /// Element type, e.g. `"button"`, `"input"`, `"h1"`, `"text"`, `"data_grid"`.
    pub kind: String,
    /// Human-readable label or heading text.
    pub label: Option<String>,
    /// Current value (for inputs).
    pub value: Option<String>,
    /// Machine-readable action name (`AiMeta::action`).
    pub action: Option<String>,
    /// Key-value parameters the action expects.
    pub params: Vec<(String, String)>,
    /// Human-readable description of the element's purpose.
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Reactive router — thread-local so it works in WASM and native contexts
// without locking.
// ---------------------------------------------------------------------------

thread_local! {
    static ROUTER: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
}

fn router() -> Signal<String> {
    ROUTER.with(|r| {
        r.borrow_mut()
            .get_or_insert_with(|| Signal::new("/".to_string()))
            .clone()
    })
}

/// Returns the underlying route [`Signal`] so effects can subscribe to route
/// changes (e.g. to rebuild the UI tree on navigation).
///
/// # Example
///
/// ```ignore
/// let _handle = create_effect(|| {
///     let route = route_signal().get();
///     // rebuild UI based on route
/// });
/// ```
pub fn route_signal() -> Signal<String> {
    router()
}

/// Returns the current route path.
pub fn current_route() -> String {
    router().get()
}

/// Programmatically navigate to a new route. Updates the reactive route signal,
/// causing any subscribed effects to re-run automatically.
pub fn navigate_to(route: &str) {
    router().set(route.to_string());
}

// ---------------------------------------------------------------------------
// query_state
// ---------------------------------------------------------------------------

/// Walk the UI tree and return a structured [`AgentState`] describing every
/// interactive and data-bearing element from an AI agent's perspective.
///
/// Interactive elements (buttons, inputs) include their `ai_action` name and
/// parameters so an agent can invoke [`trigger_event`] with the matching name.
pub fn query_state<Msg>(ui: &UITree<Msg>) -> AgentState {
    let mut interactive = Vec::new();
    let mut data = Vec::new();
    walk(ui, &mut interactive, &mut data);
    AgentState {
        interactive_elements: interactive,
        data_elements: data,
        current_route: current_route(),
    }
}

fn walk<Msg>(
    node: &UITree<Msg>,
    interactive: &mut Vec<ElementSummary>,
    data: &mut Vec<ElementSummary>,
) {
    let id = node.meta.data_appfront_id;
    let ai = &node.meta.ai;

    match &node.kind {
        NodeKind::Button { label } => {
            interactive.push(ElementSummary {
                id,
                kind: "button".into(),
                label: Some(label.clone()),
                value: None,
                action: ai.action.clone(),
                params: ai.params.clone(),
                description: ai.description.clone(),
            });
        }
        NodeKind::Input { value } => {
            interactive.push(ElementSummary {
                id,
                kind: "input".into(),
                label: None,
                value: Some(value.clone()),
                action: ai.action.clone(),
                params: ai.params.clone(),
                description: ai.description.clone(),
            });
        }
        NodeKind::Heading { level, text } => {
            data.push(ElementSummary {
                id,
                kind: format!("h{level}"),
                label: Some(text.clone()),
                value: None,
                action: None,
                params: Vec::new(),
                description: None,
            });
        }
        NodeKind::Text { text } => {
            data.push(ElementSummary {
                id,
                kind: "text".into(),
                label: Some(text.clone()),
                value: None,
                action: None,
                params: Vec::new(),
                description: None,
            });
        }
        NodeKind::Container { children } => {
            for child in children {
                walk(child, interactive, data);
            }
        }
        NodeKind::List { items } => {
            for item in items {
                walk(item, interactive, data);
            }
        }
        NodeKind::DataGrid { columns, rows } => {
            data.push(ElementSummary {
                id,
                kind: "data_grid".into(),
                label: Some(format!(
                    "[{}] — {} rows",
                    columns.join(", "),
                    rows.len()
                )),
                value: None,
                action: None,
                params: Vec::new(),
                description: None,
            });
        }
        NodeKind::Portal { content, .. } => {
            // Surface the portal's content so agents observe its elements.
            walk(content, interactive, data);
        }
    }
}

// ---------------------------------------------------------------------------
// trigger_event
// ---------------------------------------------------------------------------

/// Find a UI element whose `ai_action` matches `action` and dispatch its
/// `on_click` message via the provided `dispatch` callback.
///
/// Returns `true` if a matching node with a message handler was found and
/// dispatched; returns `false` otherwise.
pub fn trigger_event<Msg>(
    ui: &UITree<Msg>,
    action: &str,
    dispatch: &dyn Fn(Msg),
) -> bool
where
    Msg: Clone,
{
    find_and_dispatch(ui, action, dispatch)
}

fn find_and_dispatch<Msg>(
    node: &UITree<Msg>,
    action: &str,
    dispatch: &dyn Fn(Msg),
) -> bool
where
    Msg: Clone,
{
    if node.meta.ai.action.as_deref() == Some(action) {
        if let Some(msg) = &node.meta.on_click {
            dispatch(msg.clone());
            return true;
        }
    }
    match &node.kind {
        NodeKind::Container { children } => {
            for child in children {
                if find_and_dispatch(child, action, dispatch) {
                    return true;
                }
            }
        }
        NodeKind::List { items } => {
            for item in items {
                if find_and_dispatch(item, action, dispatch) {
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;


    #[derive(Debug, Clone, PartialEq)]
    enum TestMsg {
        Submit,
    }

    fn sample_ui() -> UITree<TestMsg> {
        UITree::container(|c| {
            c.heading(1, "Dashboard").class("text-2xl font-bold");
            c.button("Export")
                .on_click(TestMsg::Submit)
                .ai_action("export_data")
                .ai_param("format", "csv")
                .ai_description("Export the data as CSV");
            c.input("hello").ai_action("search").ai_param("key", "query");
            c.list(|l| {
                l.text("Item A");
                l.text("Item B");
            });
            c.data_grid(["A", "B"], [vec!["1", "2"], vec!["3", "4"]]);
        })
    }

    #[test]
    fn query_state_collects_interactive_and_data() {
        let ui = sample_ui();
        let state = query_state(&ui);

        // Interactive: button + input
        assert_eq!(state.interactive_elements.len(), 2);

        let btn = &state.interactive_elements[0];
        assert_eq!(btn.kind, "button");
        assert_eq!(btn.label.as_deref(), Some("Export"));
        assert_eq!(btn.action.as_deref(), Some("export_data"));
        assert_eq!(btn.params, vec![("format".into(), "csv".into())]);
        assert_eq!(btn.description.as_deref(), Some("Export the data as CSV"));

        let input = &state.interactive_elements[1];
        assert_eq!(input.kind, "input");
        assert_eq!(input.value.as_deref(), Some("hello"));
        assert_eq!(input.action.as_deref(), Some("search"));

        // Data: heading + list items (2) + data_grid = 4
        assert_eq!(state.data_elements.len(), 4);
        assert_eq!(state.data_elements[0].kind, "h1");
        assert_eq!(state.data_elements[0].label.as_deref(), Some("Dashboard"));
        assert_eq!(state.data_elements[1].kind, "text");
        assert_eq!(state.data_elements[1].label.as_deref(), Some("Item A"));
        assert_eq!(state.data_elements[2].kind, "text");
        assert_eq!(state.data_elements[2].label.as_deref(), Some("Item B"));
        assert_eq!(state.data_elements[3].kind, "data_grid");
        assert_eq!(
            state.data_elements[3].label.as_deref(),
            Some("[A, B] — 2 rows")
        );

        // Route defaults to "/"
        assert_eq!(state.current_route, "/");
    }

    #[test]
    fn trigger_event_dispatches_matching_action() {
        let mut ui = sample_ui();
        ui.assign_ids();

        let dispatched = std::cell::Cell::new(None::<TestMsg>);
        let dispatch = |msg: TestMsg| {
            dispatched.set(Some(msg));
        };

        let result = trigger_event(&ui, "export_data", &dispatch);
        assert!(result, "should find and dispatch the event");
        assert_eq!(dispatched.take(), Some(TestMsg::Submit));
    }

    #[test]
    fn trigger_event_returns_false_for_unknown_action() {
        let ui = sample_ui();
        let result = trigger_event(&ui, "nonexistent", &|_: TestMsg| {});
        assert!(!result, "unknown action should return false");
    }

    #[test]
    fn trigger_event_returns_false_when_no_on_click() {
        // The input has ai_action("search") but no .on_click()
        let ui = sample_ui();
        let result = trigger_event(&ui, "search", &|_: TestMsg| {});
        assert!(!result, "action without on_click should return false");
    }

    #[test]
    fn navigate_updates_route() {
        let before = current_route();
        assert_eq!(before, "/", "default route is /");

        navigate_to("/dashboard");
        assert_eq!(current_route(), "/dashboard");

        navigate_to("/settings");
        assert_eq!(current_route(), "/settings");
    }

    #[test]
    fn route_signal_is_reactive() {
        use crate::signal::create_effect;
        use std::rc::Rc;

        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let route = route_signal();

        let seen_clone = Rc::clone(&seen);
        let _handle = create_effect(move || {
            let r = route.get();
            seen_clone.borrow_mut().push(r);
        });

        // Effect runs once immediately with "/"
        assert_eq!(seen.borrow().len(), 1);

        navigate_to("/foo");
        // Effect re-runs because navigate_to sets the signal
        assert_eq!(seen.borrow().len(), 2);
        assert_eq!(seen.borrow()[1], "/foo");
    }

    #[test]
    fn query_state_respects_assign_ids() {
        let mut ui = sample_ui();
        ui.assign_ids();

        let state = query_state(&ui);

        // All elements should have ids assigned
        for el in &state.interactive_elements {
            assert!(el.id.is_some(), "interactive element should have id");
        }
        for el in &state.data_elements {
            assert!(el.id.is_some(), "data element should have id");
        }

        // Button gets id 3 (root=1, heading=2, button=3, input=4, list=5, itemA=6, itemB=7, grid=8)
        assert_eq!(state.interactive_elements[0].id, Some(3));
    }

    #[test]
    fn query_state_flat_list_and_container() {
        let mut ui: UITree<TestMsg> = UITree::container(|c| {
            c.container(|inner| {
                inner.button("Nested").ai_action("nested_btn");
            });
            c.list(|l| {
                l.button("List button").ai_action("list_btn");
            });
        });
        ui.assign_ids();

        let state = query_state(&ui);

        // Should find both buttons (one nested in container, one in list)
        assert_eq!(state.interactive_elements.len(), 2);
        assert_eq!(state.interactive_elements[0].action.as_deref(), Some("nested_btn"));
        assert_eq!(state.interactive_elements[1].action.as_deref(), Some("list_btn"));
    }
}
