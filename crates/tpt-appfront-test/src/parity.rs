//! Cross-backend text parity: does each backend show the words the tree declares?
//!
//! The expected words come from the core tree: heading, text, button, checkbox and link text, and
//! select or radio option labels. For each backend the helper collects the words the backend
//! actually outputs, then reports the difference.
//!
//! - HTML and the terminal UI must show every expected word. These two render the whole tree, so
//!   [`assert_parity`] fails when either drops one.
//! - The AI schema is a structured summary, not a rendering. Its gaps are reported by
//!   [`parity_gaps`], and [`assert_parity`] does not check them.
//! - The canvas is not checked here. It paints images, links and media as captions, and it has
//!   no visible text output to compare against.

use std::collections::BTreeSet;

use tpt_appfront_core::{NodeKind, UITree};

/// Words a backend dropped, or did not show, compared with the core tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendGap {
    /// `"html"`, `"tui"` or `"ai-schema"`.
    pub backend: &'static str,
    /// Expected words that the backend's output does not contain, in sorted order.
    pub missing: Vec<String>,
}

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
        | NodeKind::Media { .. } => {}
    }
}

/// Removes markup from HTML and decodes the entities the HTML backend emits. Attribute values
/// sit inside tags, so they are removed along with the tags, and only visible text remains.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// Words the HTML backend shows.
pub fn html_words<Msg>(ui: &UITree<Msg>) -> BTreeSet<String> {
    words(&strip_html(&tpt_appfront_html::render(ui)))
}

/// Words the terminal backend draws. The buffer is large enough that no text is truncated.
pub fn tui_words<Msg: Clone>(ui: &UITree<Msg>) -> BTreeSet<String> {
    let buffer = tpt_appfront_tui::render_to_buffer(ui, 200, 200);
    words(&tpt_appfront_tui::buffer_to_string(&buffer))
}

/// Every string in the AI schema's JSON, as words.
pub fn ai_schema_words<Msg>(ui: &UITree<Msg>) -> BTreeSet<String> {
    fn strings(value: &serde_json::Value, out: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::String(s) => out.extend(words(s)),
            serde_json::Value::Array(items) => {
                for item in items {
                    strings(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for v in map.values() {
                    strings(v, out);
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    if let Ok(value) = tpt_appfront_ai_schema::to_ai_schema_value(ui) {
        strings(&value, &mut out);
    }
    out
}

fn gap(backend: &'static str, expected: &BTreeSet<String>, shown: &BTreeSet<String>) -> BackendGap {
    BackendGap {
        backend,
        missing: expected.difference(shown).cloned().collect(),
    }
}

/// Reports, for each backend, which expected words it does not show.
pub fn parity_gaps<Msg: Clone>(ui: &UITree<Msg>) -> Vec<BackendGap> {
    let expected = expected_words(ui);
    vec![
        gap("html", &expected, &html_words(ui)),
        gap("tui", &expected, &tui_words(ui)),
        gap("ai-schema", &expected, &ai_schema_words(ui)),
    ]
}

/// Panics when the HTML or the terminal backend drops required text (see [`required_words`]). The
/// AI schema is reported by [`parity_gaps`] but not checked here.
pub fn assert_parity<Msg: Clone>(ui: &UITree<Msg>) {
    let required = required_words(ui);
    let failures: Vec<String> = parity_gaps(ui)
        .into_iter()
        .filter(|g| g.backend != "ai-schema")
        .filter_map(|g| {
            let missing: Vec<String> = g
                .missing
                .into_iter()
                .filter(|w| required.contains(w))
                .collect();
            (!missing.is_empty()).then(|| format!("  {}: missing {:?}", g.backend, missing))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "backends do not show all declared text:\n{}",
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use tpt_appfront_core::ContainerBuilder;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Go,
    }

    fn sample() -> UITree<Msg> {
        UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Welcome & Thanks");
            c.text("Count: 3");
            c.button("Go").on_click(Msg::Go);
        })
    }

    #[test]
    fn words_split_on_punctuation_and_lowercase() {
        let w = words("Count: 3, Go!");
        assert_eq!(
            w.iter().map(String::as_str).collect::<Vec<_>>(),
            ["3", "count", "go"]
        );
    }

    #[test]
    fn expected_words_come_from_visible_text_only() {
        let expected = expected_words(&sample());
        for word in ["welcome", "thanks", "count", "3", "go"] {
            assert!(expected.contains(word), "expected `{word}` in {expected:?}");
        }
    }

    #[test]
    fn html_strip_removes_attributes_and_decodes_entities() {
        let visible = strip_html("<h1 aria-label=\"hidden\">Ada &amp; Grace</h1>");
        let w = words(&visible);
        assert!(w.contains("ada") && w.contains("grace"));
        assert!(
            !w.contains("hidden"),
            "attribute text is not visible: {w:?}"
        );
    }

    #[test]
    fn html_and_tui_show_the_sample_text() {
        assert_parity(&sample());
    }

    #[test]
    fn unselected_options_are_reported_but_not_required() {
        use tpt_appfront_core::ContainerBuilder as B;
        let ui: UITree<Msg> = UITree::container(|c: &mut B<Msg>| {
            c.heading(1, "Settings");
            c.select(
                vec![
                    ("de".to_string(), "German".to_string()),
                    ("en".to_string(), "English".to_string()),
                ],
                "en",
            );
        });
        // The terminal shows only the selected option, so `german` is a reported gap ...
        let tui = parity_gaps(&ui)
            .into_iter()
            .find(|g| g.backend == "tui")
            .unwrap();
        assert!(
            tui.missing.contains(&"german".to_string()),
            "{:?}",
            tui.missing
        );
        // ... but it is not a required word, so the parity assertion passes.
        assert!(!required_words(&ui).contains("german"));
        assert_parity(&ui);
    }

    #[test]
    fn gaps_report_every_backend_and_ai_schema_is_not_asserted() {
        let gaps = parity_gaps(&sample());
        let names: Vec<&str> = gaps.iter().map(|g| g.backend).collect();
        assert_eq!(names, ["html", "tui", "ai-schema"]);
        // The sample's visible words must reach the HTML and TUI outputs.
        assert!(
            gaps[0].missing.is_empty(),
            "html gaps: {:?}",
            gaps[0].missing
        );
        assert!(
            gaps[1].missing.is_empty(),
            "tui gaps: {:?}",
            gaps[1].missing
        );
    }
}
