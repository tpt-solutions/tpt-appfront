//! Human-facing devtools inspector for the `UITree`.
//!
//! Reuses the AI-agent [`AgentState`]/[`ElementSummary`] snapshot (see
//! [`crate::agent`]) plus a pretty-printed view of the tree itself, so a
//! developer can inspect the structure, the interactive/data elements an AI
//! agent would see, and — when signals are named via
//! [`crate::signal::Signal::labeled`] — which signals have been firing.
//!
//! The output is plain text (ideal for a terminal/console devtools panel) and
//! also renderable to a self-contained HTML snippet via [`to_html`].

use crate::agent::{AgentState, ElementSummary};
use crate::signal::signal_activity;
use crate::ui_tree::{NodeKind, UITree};

/// A complete devtools report: the tree view, the agent-state view, and the
/// signal-activity view.
#[derive(Debug, Clone)]
pub struct DevtoolsReport {
    /// Pretty-printed `UITree` with per-node metadata annotations.
    pub tree: String,
    /// Human-readable listing of the `AgentState` snapshot.
    pub state: String,
    /// Per-label signal-write counts (empty when no signals are labeled).
    pub signals: String,
}

/// Pretty-prints a `UITree` as an indented tree, annotating each node with its
/// `meta` (class, key, dynamic flag, AI action, on_click presence, assigned id).
pub fn inspect_tree<Msg>(ui: &UITree<Msg>) -> String {
    let mut out = String::new();
    write_node(ui, &mut out, "", true);
    out
}

fn node_header<Msg>(ui: &UITree<Msg>) -> String {
    match &ui.kind {
        NodeKind::Container { children } => format!("Container ({} children)", children.len()),
        NodeKind::Heading { level, text } => format!("h{level} \"{text}\""),
        NodeKind::Text { text } => format!("Text \"{text}\""),
        NodeKind::Button { label } => format!("Button \"{label}\""),
        NodeKind::Input { value } => format!("Input value=\"{value}\""),
        NodeKind::Textarea { value } => format!("Textarea value=\"{value}\""),
        NodeKind::Checkbox { label, checked } => {
            format!("Checkbox \"{label}\" checked={checked}")
        }
        NodeKind::Select { options, selected } => format!(
            "Select [{options:?}] selected=\"{selected}\""
        ),
        NodeKind::Radio { name, options, selected } => format!(
            "Radio name=\"{name}\" [{options:?}] selected=\"{selected}\""
        ),
        NodeKind::List { items } => format!("List ({}) items", items.len()),
        NodeKind::DataGrid { columns, rows } => {
            format!("DataGrid [{}] {}x{}", columns.join(", "), columns.len(), rows.len())
        }
        NodeKind::Portal { target, .. } => format!("Portal -> \"{target}\""),
    }
}

fn node_annotations<Msg>(ui: &UITree<Msg>) -> Vec<String> {
    let meta = &ui.meta;
    let mut parts = Vec::new();
    if let Some(id) = meta.data_appfront_id {
        parts.push(format!("#{id}"));
    }
    if let Some(class) = &meta.class {
        parts.push(format!("class=\"{class}\""));
    }
    if let Some(key) = &meta.key {
        parts.push(format!("key=\"{key}\""));
    }
    if meta.is_dynamic {
        parts.push("dynamic".into());
    }
    if let Some(action) = &meta.ai.action {
        parts.push(format!("ai:{action}"));
    }
    if meta.on_click.is_some() {
        parts.push("on_click".into());
    }
    parts
}

fn node_children<Msg>(ui: &UITree<Msg>) -> &[UITree<Msg>] {
    match &ui.kind {
        NodeKind::Container { children } => children,
        NodeKind::List { items } => items,
        _ => &[],
    }
}

fn write_node<Msg>(ui: &UITree<Msg>, out: &mut String, prefix: &str, is_last: bool) {
    let branch = if is_last { "└─ " } else { "├─ " };
    out.push_str(prefix);
    out.push_str(branch);
    out.push_str(&node_header(ui));

    let annotations = node_annotations(ui);
    if !annotations.is_empty() {
        out.push_str("  ");
        out.push_str(&annotations.join(" "));
    }
    out.push('\n');

    let children = node_children(ui);
    let child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
    for (i, child) in children.iter().enumerate() {
        write_node(child, out, &child_prefix, i + 1 == children.len());
    }
}

/// Renders an [`AgentState`] snapshot as a readable listing of the
/// interactive and data elements an AI agent would observe.
pub fn inspect_state(state: &AgentState) -> String {
    let mut out = String::new();
    out.push_str(&format!("route: {}\n", state.current_route));
    out.push_str(&format!(
        "interactive elements ({}):\n",
        state.interactive_elements.len()
    ));
    for el in &state.interactive_elements {
        out.push_str("  - ");
        out.push_str(&element_line(el));
        out.push('\n');
    }
    out.push_str(&format!("data elements ({}):\n", state.data_elements.len()));
    for el in &state.data_elements {
        out.push_str("  - ");
        out.push_str(&element_line(el));
        out.push('\n');
    }
    out
}

fn element_line(el: &ElementSummary) -> String {
    let mut s = el.kind.clone();
    if let Some(label) = &el.label {
        s.push_str(&format!(" \"{label}\""));
    }
    if let Some(value) = &el.value {
        s.push_str(&format!(" value=\"{value}\""));
    }
    if let Some(action) = &el.action {
        s.push_str(&format!(" action={action}"));
    }
    let params: Vec<String> = el
        .params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if !params.is_empty() {
        s.push_str(&format!(" ({})", params.join(", ")));
    }
    if let Some(desc) = &el.description {
        s.push_str(&format!(" — {desc}"));
    }
    s
}

/// Builds a full [`DevtoolsReport`] from a `UITree` and its [`AgentState`].
pub fn render<Msg>(ui: &UITree<Msg>, state: &AgentState) -> DevtoolsReport {
    let activity = signal_activity();
    let signals = if activity.is_empty() {
        "(no labeled signals — name signals with `Signal::labeled` to track writes)".into()
    } else {
        let mut entries: Vec<(&String, &u64)> = activity.iter().collect();
        entries.sort_by_key(|(name, _)| *name);
        entries
            .into_iter()
            .map(|(name, count)| format!("  - {name}: {count} write(s)"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    DevtoolsReport {
        tree: inspect_tree(ui),
        state: inspect_state(state),
        signals,
    }
}

/// Renders a [`DevtoolsReport`] as a self-contained HTML snippet suitable for
/// embedding in a devtools panel (no external CSS/JS).
pub fn to_html(report: &DevtoolsReport) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    format!(
        "<div class=\"appfront-devtools\">\n  \
         <h3>UI Tree</h3>\n  <pre>{tree}</pre>\n  \
         <h3>Agent State</h3>\n  <pre>{state}</pre>\n  \
         <h3>Signal Activity</h3>\n  <pre>{signals}</pre>\n\
         </div>",
        tree = esc(&report.tree),
        state = esc(&report.state),
        signals = esc(&report.signals),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::query_state;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Increment,
    }

    fn sample_ui() -> UITree<Msg> {
        UITree::container(|c| {
            c.heading(1, "Dashboard").class("title");
            c.container(|inner| {
                inner
                    .button("+1")
                    .on_click(Msg::Increment)
                    .ai_action("increment")
                    .ai_description("Increment the counter");
            });
            c.input("hello world");
        })
    }

    #[test]
    fn inspect_tree_shows_structure_and_annotations() {
        let ui = sample_ui();
        let out = inspect_tree(&ui);
        assert!(out.contains("Container ("), "root container");
        assert!(out.contains("h1 \"Dashboard\""), "heading node");
        assert!(out.contains("class=\"title\""), "heading class annotation");
        assert!(out.contains("Button \"+1\""), "button node");
        assert!(out.contains("ai:increment"), "ai action annotation");
        assert!(out.contains("on_click"), "on_click annotation");
        assert!(out.contains("Input value=\"hello world\""), "input node");
        // Nested container should be indented under the root.
        assert!(out.contains("│  ") || out.contains("   "), "has indentation");
    }

    #[test]
    fn inspect_state_lists_interactive_and_data_elements() {
        let ui = sample_ui();
        let state = query_state(&ui);
        let out = inspect_state(&state);
        assert!(out.contains("interactive elements (2):"));
        assert!(out.contains("data elements (1):"));
        assert!(out.contains("action=increment"));
        assert!(out.contains("h1 \"Dashboard\""));
    }

    #[test]
    fn render_produces_a_full_report() {
        let ui = sample_ui();
        let state = query_state(&ui);
        let report = render(&ui, &state);
        assert!(report.tree.contains("Container ("));
        assert!(report.state.contains("route:"));
        // No labeled signals were used, so the fallback message is shown.
        assert!(report.signals.contains("no labeled signals"));
    }

    #[test]
    fn to_html_escapes_and_wraps_report() {
        let ui = sample_ui();
        let state = query_state(&ui);
        let report = render(&ui, &state);
        let html = to_html(&report);
        assert!(html.starts_with("<div class=\"appfront-devtools\">"));
        assert!(html.contains("<pre>"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn signal_activity_is_reported_when_labeled() {
        use crate::signal::{reset_signal_activity, Signal};

        reset_signal_activity();
        let count = Signal::new(0i32).labeled("count");
        count.set(1);
        count.set(2);
        // Setting the same value still records a write (activity is write-count).
        count.set(2);

        let ui = sample_ui();
        let state = query_state(&ui);
        let report = render(&ui, &state);
        assert!(report.signals.contains("count: 3 write(s)"), "got: {}", report.signals);
    }
}
