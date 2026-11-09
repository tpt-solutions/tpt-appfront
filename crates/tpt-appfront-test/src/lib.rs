//! Headless test harness and linter for `UITree`s.
//!
//! [`Harness`] wraps a tree and records every message a click, input or key press produces, so
//! tests can drive the UI without a browser, window or terminal. [`lint_tree`] checks a tree for
//! accessibility and AI-affordance problems (unlabeled inputs, nameless buttons, skipped heading
//! levels, duplicate keys, missing image alt text and undocumented AI actions).

use std::collections::HashSet;
use std::fmt::Debug;

use tpt_appfront_core::devtools::inspect_tree;
use tpt_appfront_core::ui_tree::KeyEvent;
use tpt_appfront_core::{NodeKind, UITree};

#[cfg(feature = "parity")]
pub mod parity;

/// The ARIA-like role of a node, as the harness reports it.
pub fn role<Msg>(ui: &UITree<Msg>) -> &'static str {
    match &ui.kind {
        NodeKind::Container { .. } => "group",
        NodeKind::Heading { .. } => "heading",
        NodeKind::Text { .. } => "text",
        NodeKind::Button { .. } => "button",
        NodeKind::Input { .. } | NodeKind::Textarea { .. } => "textbox",
        NodeKind::Checkbox { .. } => "checkbox",
        NodeKind::Select { .. } => "combobox",
        NodeKind::Radio { .. } => "radio",
        NodeKind::List { .. } => "list",
        NodeKind::DataGrid { .. } => "table",
        NodeKind::Portal { .. } => "group",
        NodeKind::Image { .. } => "img",
        NodeKind::Link { .. } => "link",
        NodeKind::Media { .. } => "media",
    }
}

/// The text a node presents to a user: heading/text content, a button label, a checkbox label or
/// a link's text. `None` for nodes with no text of their own.
pub fn text_of<Msg>(ui: &UITree<Msg>) -> Option<&str> {
    match &ui.kind {
        NodeKind::Heading { text, .. } | NodeKind::Text { text } => Some(text),
        NodeKind::Button { label } | NodeKind::Checkbox { label, .. } => Some(label),
        NodeKind::Link { text, .. } => Some(text),
        _ => None,
    }
}

/// Every node in `ui` in document order (the node itself, then its children), including nodes
/// inside lists and portals.
pub fn walk<Msg>(ui: &UITree<Msg>) -> Vec<&UITree<Msg>> {
    let mut out = Vec::new();
    collect(ui, &mut out);
    out
}

fn collect<'a, Msg>(ui: &'a UITree<Msg>, out: &mut Vec<&'a UITree<Msg>>) {
    out.push(ui);
    match &ui.kind {
        NodeKind::Container { children } | NodeKind::List { items: children } => {
            for child in children {
                collect(child, out);
            }
        }
        NodeKind::Portal { content, .. } => collect(content, out),
        _ => {}
    }
}

/// A headless view of a `UITree` for tests.
///
/// Lookups panic with a message naming what was missing, which reads well in a failing test.
/// Message-producing actions queue the message; read them back with [`Harness::drain_msgs`].
pub struct Harness<Msg> {
    root: UITree<Msg>,
    msgs: Vec<Msg>,
}

impl<Msg: Clone + Debug + PartialEq> Harness<Msg> {
    /// Wraps `root` for testing.
    pub fn new(root: UITree<Msg>) -> Self {
        Self {
            root,
            msgs: Vec::new(),
        }
    }

    /// Replaces the tree, as a re-render would. Queued messages are kept.
    pub fn rerender(&mut self, root: UITree<Msg>) {
        self.root = root;
    }

    /// The tree under test.
    pub fn tree(&self) -> &UITree<Msg> {
        &self.root
    }

    /// The devtools text rendering of the tree, for snapshot-style assertions.
    pub fn snapshot(&self) -> String {
        inspect_tree(&self.root)
    }

    /// The first node (in document order) whose [`text_of`] equals `text`.
    pub fn find_by_text(&self, text: &str) -> Option<&UITree<Msg>> {
        walk(&self.root)
            .into_iter()
            .find(|n| text_of(n) == Some(text))
    }

    /// The first node with the given role.
    pub fn find_by_role(&self, role_name: &str) -> Option<&UITree<Msg>> {
        walk(&self.root).into_iter().find(|n| role(n) == role_name)
    }

    /// The node carrying the explicit `key` (see `NodeRef::key`).
    pub fn find_by_key(&self, key: &str) -> Option<&UITree<Msg>> {
        walk(&self.root)
            .into_iter()
            .find(|n| n.meta.key.as_deref() == Some(key))
    }

    /// Clicks the node whose text is `text`, queueing its `on_click` message.
    ///
    /// # Panics
    /// If no node has that text, or the node has no `on_click` handler.
    pub fn click(&mut self, text: &str) {
        let node = self
            .find_by_text(text)
            .unwrap_or_else(|| panic!("click: no node with text {text:?}"));
        match node.meta.on_click.clone() {
            Some(msg) => self.msgs.push(msg),
            None => panic!("click: node {text:?} has no on_click handler"),
        }
    }

    /// Types `value` into the input with `key`, queueing the message its `on_input` produces.
    ///
    /// # Panics
    /// If no node has that key, or the node has no `on_input` handler.
    pub fn type_into(&mut self, key: &str, value: &str) {
        let node = self
            .find_by_key(key)
            .unwrap_or_else(|| panic!("type_into: no node with key {key:?}"));
        match &node.meta.on_input {
            Some(on_input) => self.msgs.push(on_input(value.to_string())),
            None => panic!("type_into: node {key:?} has no on_input handler"),
        }
    }

    /// Presses `key` (a `KeyboardEvent.key` name such as `"Enter"`) on the node with `node_key`,
    /// queueing the message its `on_keydown` produces, if any.
    ///
    /// # Panics
    /// If no node has that key, or the node has no `on_keydown` handler.
    pub fn press_key(&mut self, node_key: &str, key: &str) {
        let node = self
            .find_by_key(node_key)
            .unwrap_or_else(|| panic!("press_key: no node with key {node_key:?}"));
        let Some(on_keydown) = node.meta.on_keydown.clone() else {
            panic!("press_key: node {node_key:?} has no on_keydown handler");
        };
        let event = KeyEvent {
            key: key.to_string(),
            code: String::new(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            repeat: false,
        };
        if let Some(msg) = on_keydown(event) {
            self.msgs.push(msg);
        }
    }

    /// Takes every message queued so far, oldest first.
    pub fn drain_msgs(&mut self) -> Vec<Msg> {
        std::mem::take(&mut self.msgs)
    }

    /// Asserts `expected` is among the queued messages (leaving them queued).
    ///
    /// # Panics
    /// If it was not emitted; the panic lists what was.
    pub fn assert_msg_emitted(&self, expected: &Msg) {
        assert!(
            self.msgs.contains(expected),
            "expected message {expected:?} to be emitted; emitted: {:?}",
            self.msgs
        );
    }

    /// Lints the tree. See [`lint_tree`].
    pub fn lint(&self) -> Vec<LintIssue> {
        lint_tree(&self.root)
    }
}

/// One problem found by [`lint_tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintIssue {
    /// Short rule id, for example `"unlabeled-input"`.
    pub rule: &'static str,
    /// The node's explicit key, when it has one, so the issue can be located.
    pub key: Option<String>,
    /// What is wrong and how to fix it.
    pub message: String,
}

/// Checks a tree for accessibility and AI-affordance problems.
///
/// Rules:
/// - `unlabeled-input`: an input, textarea, select or radio without an accessible name
///   (`aria-label`, `aria-labelledby`, `title`, or an AI title).
/// - `nameless-button`: a button with no label text and no `aria-label`.
/// - `heading-skip`: a heading more than one level below the previous heading in document order.
/// - `duplicate-key`: two nodes share an explicit key.
/// - `missing-alt`: an image with empty alt text.
/// - `action-without-description`: an AI action with no description, so agents cannot tell what
///   it does.
pub fn lint_tree<Msg>(root: &UITree<Msg>) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let mut seen_keys: HashSet<&str> = HashSet::new();
    let mut last_heading: Option<u8> = None;

    for node in walk(root) {
        let key = node.meta.key.as_deref();
        let attr = |name: &str| {
            node.meta
                .attrs
                .iter()
                .any(|(n, v)| n == name && !v.trim().is_empty())
        };
        let mut push = |rule: &'static str, message: String| {
            issues.push(LintIssue {
                rule,
                key: key.map(str::to_owned),
                message,
            });
        };

        if let Some(k) = key {
            if !seen_keys.insert(k) {
                push("duplicate-key", format!("key {k:?} is used more than once"));
            }
        }

        match &node.kind {
            NodeKind::Input { .. }
            | NodeKind::Textarea { .. }
            | NodeKind::Select { .. }
            | NodeKind::Radio { .. } => {
                let named = attr("aria-label")
                    || attr("aria-labelledby")
                    || attr("title")
                    || node
                        .meta
                        .ai
                        .title
                        .as_deref()
                        .is_some_and(|t| !t.trim().is_empty());
                if !named {
                    push(
                        "unlabeled-input",
                        format!(
                            "{} has no accessible name; add aria-label or a title",
                            role(node)
                        ),
                    );
                }
            }
            NodeKind::Button { label } => {
                if label.trim().is_empty() && !attr("aria-label") {
                    push(
                        "nameless-button",
                        "button has no label text and no aria-label".to_string(),
                    );
                }
            }
            NodeKind::Heading { level, .. } => {
                if let Some(prev) = last_heading {
                    if *level > prev + 1 {
                        push(
                            "heading-skip",
                            format!("heading level jumps from h{prev} to h{level}"),
                        );
                    }
                }
                last_heading = Some(*level);
            }
            NodeKind::Image { alt, .. } if alt.trim().is_empty() => {
                push("missing-alt", "image has empty alt text".to_string());
            }
            _ => {}
        }

        if let Some(action) = &node.meta.ai.action {
            let described = node
                .meta
                .ai
                .description
                .as_deref()
                .is_some_and(|d| !d.trim().is_empty());
            if !described {
                push(
                    "action-without-description",
                    format!("AI action `{action}` has no description"),
                );
            }
        }
    }
    issues
}

/// Panics with every issue [`lint_tree`] finds. Put it in a test, so an accessibility regression
/// fails the build the same way a broken assertion does.
///
/// ```ignore
/// #[test]
/// fn home_page_is_accessible() {
///     tpt_appfront_test::assert_accessible(&home_view());
/// }
/// ```
pub fn assert_accessible<Msg>(root: &UITree<Msg>) {
    let issues = lint_tree(root);
    if issues.is_empty() {
        return;
    }
    let lines: Vec<String> = issues
        .iter()
        .map(|issue| match &issue.key {
            Some(key) => format!("  [{}] node {key:?}: {}", issue.rule, issue.message),
            None => format!("  [{}] {}", issue.rule, issue.message),
        })
        .collect();
    panic!(
        "the view has {} accessibility issue(s):\n{}",
        issues.len(),
        lines.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use tpt_appfront_core::ContainerBuilder;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Increment,
        Name(String),
        Submit,
    }

    fn counter() -> UITree<Msg> {
        UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Counter");
            c.button("Increment").on_click(Msg::Increment);
            c.input("")
                .key("name")
                .attr("aria-label", "Name")
                .on_input(Msg::Name);
            c.button("Submit").on_click(Msg::Submit).key("submit");
        })
    }

    #[test]
    fn click_queues_the_handler_message() {
        let mut h = Harness::new(counter());
        h.click("Increment");
        assert_eq!(h.drain_msgs(), vec![Msg::Increment]);
        assert!(h.drain_msgs().is_empty(), "drain empties the queue");
    }

    #[test]
    fn type_into_queues_the_input_message() {
        let mut h = Harness::new(counter());
        h.type_into("name", "ada");
        h.assert_msg_emitted(&Msg::Name("ada".into()));
    }

    #[test]
    fn finds_nodes_by_text_role_and_key() {
        let h = Harness::new(counter());
        assert_eq!(role(h.find_by_text("Counter").unwrap()), "heading");
        assert_eq!(
            h.find_by_role("textbox").unwrap().meta.key.as_deref(),
            Some("name")
        );
        assert!(h.find_by_key("submit").is_some());
        assert!(h.find_by_text("Missing").is_none());
    }

    #[test]
    #[should_panic(expected = "no node with text \"Nope\"")]
    fn click_panics_naming_a_missing_node() {
        Harness::new(counter()).click("Nope");
    }

    #[test]
    #[should_panic(expected = "has no on_click handler")]
    fn click_panics_on_a_node_without_a_handler() {
        let mut h = Harness::new(UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.button("Inert");
        }));
        h.click("Inert");
    }

    #[test]
    #[should_panic(expected = "expected message Submit to be emitted")]
    fn assert_msg_emitted_panics_when_absent() {
        Harness::new(counter()).assert_msg_emitted(&Msg::Submit);
    }

    #[test]
    fn press_key_uses_the_keydown_handler() {
        let tree = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.button("Go")
                .key("go")
                .on_keydown(|k| (k.key == "Enter").then_some(Msg::Submit));
        });
        let mut h = Harness::new(tree);
        h.press_key("go", "a");
        assert!(h.drain_msgs().is_empty());
        h.press_key("go", "Enter");
        assert_eq!(h.drain_msgs(), vec![Msg::Submit]);
    }

    #[test]
    fn snapshot_is_the_devtools_text_tree() {
        let h = Harness::new(counter());
        assert!(h.snapshot().contains("Counter"));
    }

    #[test]
    fn lint_flags_each_rule() {
        let tree: UITree<Msg> = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Title");
            c.heading(3, "Skipped");
            c.input("").key("dup");
            c.input("").key("dup").attr("title", "Ok");
            c.button("  ");
            c.image("pic.png", "");
            c.button("Do it").ai_action("do_it");
        });
        let rules: Vec<&str> = lint_tree(&tree).iter().map(|i| i.rule).collect();
        for rule in [
            "heading-skip",
            "unlabeled-input",
            "duplicate-key",
            "nameless-button",
            "missing-alt",
            "action-without-description",
        ] {
            assert!(rules.contains(&rule), "expected `{rule}` in {rules:?}");
        }
    }

    #[test]
    fn assert_accessible_passes_a_clean_view() {
        let tree: UITree<Msg> = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Hi");
            c.button("Go").on_click(Msg::Submit);
        });
        assert_accessible(&tree);
    }

    #[test]
    #[should_panic(expected = "[unlabeled-input]")]
    fn assert_accessible_reports_each_rule_it_fails() {
        let tree: UITree<Msg> = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.input("");
        });
        assert_accessible(&tree);
    }

    #[test]
    fn lint_passes_a_clean_tree() {
        let tree: UITree<Msg> = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Title");
            c.heading(2, "Section");
            c.input("").attr("aria-label", "Search");
            c.image("pic.png", "A picture");
            c.button("Save")
                .ai_action("save")
                .ai_description("Saves the form");
        });
        let issues = lint_tree(&tree);
        assert!(issues.is_empty(), "{issues:?}");
    }
}
