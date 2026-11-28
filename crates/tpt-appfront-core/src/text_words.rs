//! Visible-text words of a `UITree`: the words a backend is expected to show.
//!
//! Moved here from `tpt_appfront_test::parity` so that a wasm test can use it. The
//! parity helper pulls in the TUI backend, and that does not build for wasm32.
//! `tpt_appfront_test::parity` re-exports these functions unchanged.

use std::collections::BTreeSet;

use crate::{NodeKind, UITree};

/// Splits `text` into lower-cased words. Any run of characters that are not letters or digits
/// separates words, so `Count: 0` gives `count` and `0`.
pub fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The visible text the core tree declares, as words. Option labels of selects and radio groups
/// are included.
pub fn expected_words<Msg>(ui: &UITree<Msg>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_expected(ui, true, &mut out);
    out
}

/// The text every backend is required to show. Option labels are left out: a terminal shows only
/// the selected option of a select, which is a design choice, not a lost word.
pub fn required_words<Msg>(ui: &UITree<Msg>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_expected(ui, false, &mut out);
    out
}

fn collect_expected<Msg>(ui: &UITree<Msg>, include_options: bool, out: &mut BTreeSet<String>) {
    let mut add = |text: &str| out.extend(words(text));
    match &ui.kind {
        NodeKind::Container { children } => {
            for child in children {
                collect_expected(child, include_options, out);
            }
        }
        NodeKind::List { items } => {
            for item in items {
                collect_expected(item, include_options, out);
            }
        }
        NodeKind::Portal { content, .. } => collect_expected(content, include_options, out),
        NodeKind::Heading { text, .. } | NodeKind::Text { text } | NodeKind::Link { text, .. } => {
            add(text)
        }
        NodeKind::Button { label } | NodeKind::Checkbox { label, .. } => add(label),
        NodeKind::Select { options, .. } | NodeKind::Radio { options, .. } => {
            if include_options {
                for (_, label) in options {
                    add(label);
                }
            }
        }
        NodeKind::Input { .. }
        | NodeKind::Textarea { .. }
        | NodeKind::DataGrid { .. }
        | NodeKind::Image { .. }
        | NodeKind::Viewport { .. }
        | NodeKind::Media { .. } => {}
    }
}
