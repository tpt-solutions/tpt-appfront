//! `tpt-appfront ingest <input.html> [--out <file>]` — converts existing
//! static / server-rendered HTML into a `view!` builder-call skeleton.
//!
//! This is *structure only*: tags and classes are mapped to `view!` nodes, but
//! any inline event handler (`onclick=...`) is emitted as a `todo!()` stub
//! rather than a guessed `Msg`, and a summary of anything unmapped/dropped is
//! printed to stderr. A JSX/React SPA source is explicitly out of scope (its
//! HTML is just an empty shell `<div>`); feed this command a rendered DOM
//! snapshot instead.

use scraper::{Html, Selector};

use anyhow::Context;

/// Ingests `input_html` (a string of HTML markup) and returns a `view!` source
/// skeleton. `dropped` (if `Some`) receives a human-readable summary of any
/// elements/attributes that were recognized but not mapped (e.g. inline event
/// handlers), so the caller can print it.
pub fn ingest(input_html: &str, dropped: &mut Vec<String>) -> String {
    let doc = Html::parse_document(input_html);
    let body_sel = Selector::parse("body").unwrap();
    let root = doc
        .select(&body_sel)
        .next()
        .or_else(|| Some(doc.root_element()))
        .unwrap_or_else(|| doc.root_element());

    let mut body = String::new();
    for child in root.children() {
        let Some(node) = child.value().as_element() else {
            continue;
        };
        let Some(el) = scraper::ElementRef::wrap(child) else {
            continue;
        };
        let _ = node;
        if let Some(line) = element_to_view(el, &doc, dropped) {
            body.push_str(&line);
            body.push('\n');
        }
    }

    format!(
        "tpt_appfront_core::view! {{\n    <Container>\n{body}    </Container>\n}}\n"
    )
}

/// Maps a single HTML element to a `view!` node line (indented two levels).
/// Returns `None` for elements we intentionally skip (comments, whitespace
/// text, script/style tags). Inline event handlers are translated to
/// `todo!()`-stubbed closures and recorded in `dropped`.
fn element_to_view(
    el: scraper::ElementRef,
    _doc: &Html,
    dropped: &mut Vec<String>,
) -> Option<String> {
    let tag = el.value().name();
    match tag {
        "script" | "style" | "head" | "meta" | "link" => return None,
        _ => {}
    }

    let class = el
        .value()
        .attr("class")
        .map(|c| format!(", class=\"{}\"", c));

    // Detect inline event handlers we can't safely translate.
    let inline_handler = el
        .value()
        .attrs()
        .find(|(k, _)| k.starts_with("on") && k.len() > 2)
        .map(|(k, _)| k.to_string());

    let (open, close, self_close) = match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = tag[1..].parse::<u8>().unwrap_or(1);
            (
                format!("<Heading level={{{level}u8}}{} >", class.unwrap_or_default()),
                "</Heading>",
                false,
            )
        }
        "p" | "span" | "div" => {
            if tag == "div" {
                (
                    format!("<Container{}>", class.unwrap_or_default()),
                    "</Container>",
                    false,
                )
            } else {
                (
                    format!("<Text{}>", class.unwrap_or_default()),
                    "</Text>",
                    false,
                )
            }
        }
        "button" => (
            format!(
                "<Button on_click={{ todo!(\"wire {} onclick to a Msg\") }}{}>",
                tag,
                class.unwrap_or_default()
            ),
            "</Button>",
            false,
        ),
        "input" => {
            let value = el.value().attr("value").unwrap_or("");
            let ty = format!(", value=\"{value}\"");
            (
                format!("<Input{ty}{} />", class.unwrap_or_default()),
                "",
                true,
            )
        }
        "textarea" => (
            format!(
                "<Textarea value=\"{}\"{} />",
                el.text().collect::<String>().trim(),
                class.unwrap_or_default()
            ),
            "",
            true,
        ),
        "ul" | "ol" => (format!("<List{}>", class.unwrap_or_default()), "</List>", false),
        "table" => {
            (format!("<DataGrid{} />", class.unwrap_or_default()), "", true)
        }
        "a" => {
            let href = el.value().attr("href").unwrap_or("#");
            (
                format!("<Button on_click={{ todo!(\"link to {href}\") }}>"),
                "</Button>",
                false,
            )
        }
        other => {
            dropped.push(format!("unmapped tag `<{other}>` -> Container"));
            (
                format!("<Container{}>", class.unwrap_or_default()),
                "</Container>",
                false,
            )
        }
    };

    if let Some(handler) = inline_handler {
        dropped.push(format!(
            "inline `{handler}` on <{tag}> -> emitted as todo!() stub (behavior not guessed)"
        ));
    }

    // Gather child text / nested elements.
    let mut inner = String::new();
    for child in el.children() {
        if let Some(text) = child.value().as_text() {
            let t = text.text.trim();
            if !t.is_empty() {
                inner.push_str(&format!("\"{}\"", t.replace('"', "\\\"")));
            }
        } else if let Some(_nested_el) = child.value().as_element() {
            if let Some(nested) = scraper::ElementRef::wrap(child) {
                if let Some(line) = element_to_view(nested, _doc, dropped) {
                    inner.push('\n');
                    inner.push_str(&indent(&line, "        "));
                }
            }
        }
    }

    if self_close || inner.is_empty() {
        Some(format!("        {open}{close}"))
    } else {
        Some(format!("        {open}{inner}\n        {close}"))
    }
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{prefix}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads `path`, ingests it, writes the skeleton to `out` (or stdout), and
/// reports dropped/unmapped items to stderr.
pub fn ingest_file(path: &std::path::Path, out: Option<&std::path::Path>) -> anyhow::Result<()> {
    let html = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let mut dropped = Vec::new();
    let skeleton = ingest(&html, &mut dropped);

    match out {
        Some(p) => {
            std::fs::write(p, &skeleton).with_context(|| format!("writing {}", p.display()))?;
            println!("wrote {}", p.display());
        }
        None => print!("{skeleton}"),
    }

    if !dropped.is_empty() {
        eprintln!("note: the following were not auto-mapped:");
        for d in &dropped {
            eprintln!("  - {d}");
        }
    }
    Ok(())
}
