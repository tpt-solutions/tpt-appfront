//! Semantic HTML (SSR/SSG) backend: `UITree` → semantic HTML string.
//!
//! Produces valid HTML5 fragments or full pages with OpenGraph meta tags
//! and `data-ai-action` / `data-ai-params` attributes for AI crawlers.
//! See `docs/ai-schema.md`.

use appfront_core::{NodeKind, UITree};

/// Renders a `UITree` to a semantic HTML fragment (no `<html>`/`<head>`/`<body>`).
pub fn render<Msg>(ui: &UITree<Msg>) -> String {
    let mut buf = String::new();
    render_node(&mut buf, ui);
    buf
}

/// Renders a full HTML5 page with OpenGraph tags.
pub fn render_page<Msg>(
    ui: &UITree<Msg>,
    title: &str,
    description: &str,
) -> String {
    let body = render(ui);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta property="og:title" content="{title}">
<meta property="og:description" content="{desc}">
<meta property="og:type" content="website">
{json_ld}
</head>
<body>
{body}
</body>
</html>
"#,
        title = esc_attr(title),
        desc = esc_attr(description),
        json_ld = "",
        body = body,
    )
}

// ---------------------------------------------------------------------------
// Node rendering
// ---------------------------------------------------------------------------

fn render_node<Msg>(buf: &mut String, ui: &UITree<Msg>) {
    match &ui.kind {
        NodeKind::Container { children } => {
            open_tag(buf, "div", ui);
            for child in children {
                render_node(buf, child);
            }
            close_tag(buf, "div");
        }
        NodeKind::Heading { level, text } => {
            let tag = format!("h{}", level.clamp(&1, &6));
            open_tag(buf, &tag, ui);
            buf.push_str(&esc_text(text));
            close_tag(buf, &tag);
        }
        NodeKind::Text { text } => {
            // Only wrap in <span> if there are attributes to attach.
            if has_attrs(ui) {
                open_tag(buf, "span", ui);
                buf.push_str(&esc_text(text));
                close_tag(buf, "span");
            } else {
                buf.push_str(&esc_text(text));
            }
        }
        NodeKind::Button { label } => {
            buf.push_str("<button type=\"button\"");
            attrs(buf, ui);
            buf.push('>');
            buf.push_str(&esc_text(label));
            close_tag(buf, "button");
        }
        NodeKind::Input { value } => {
            buf.push_str("<input");
            attrs(buf, ui);
            attr(buf, "value", value);
            buf.push_str(" />");
        }
        NodeKind::List { items } => {
            open_tag(buf, "ul", ui);
            for item in items {
                buf.push_str("<li>");
                render_node(buf, item);
                buf.push_str("</li>");
            }
            close_tag(buf, "ul");
        }
        NodeKind::DataGrid { columns, rows } => {
            open_tag(buf, "table", ui);
            buf.push('>');

            // <thead>
            buf.push_str("<thead><tr>");
            for col in columns {
                buf.push_str("<th>");
                buf.push_str(&esc_text(col));
                buf.push_str("</th>");
            }
            buf.push_str("</tr></thead>");

            // <tbody>
            buf.push_str("<tbody>");
            for row in rows {
                buf.push_str("<tr>");
                for cell in row {
                    buf.push_str("<td>");
                    buf.push_str(&esc_text(cell));
                    buf.push_str("</td>");
                }
                buf.push_str("</tr>");
            }
            buf.push_str("</tbody>");

            close_tag(buf, "table");
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute helpers
// ---------------------------------------------------------------------------

fn open_tag<Msg>(buf: &mut String, tag: &str, ui: &UITree<Msg>) {
    buf.push('<');
    buf.push_str(tag);
    attrs(buf, ui);
    buf.push('>');
}

fn close_tag(buf: &mut String, tag: &str) {
    buf.push_str("</");
    buf.push_str(tag);
    buf.push('>');
}

fn attrs<Msg>(buf: &mut String, ui: &UITree<Msg>) {
    if let Some(class) = &ui.meta.class {
        attr(buf, "class", class);
        // Tailwind-style utility layer: recognized utility classes (e.g.
        // `bg-blue-500 p-4`) are resolved to real CSS so SSR output is
        // actually styled without a separate build step. See
        // `appfront_core::styling`.
        let style = appfront_core::styling::inline_style(class);
        if !style.is_empty() {
            attr(buf, "style", &style);
        }
    }
    if let Some(id) = &ui.meta.data_appfront_id {
        attr(buf, "data-appfront-id", &id.to_string());
    }
    if let Some(action) = &ui.meta.ai.action {
        attr(buf, "data-ai-action", action);
        if !ui.meta.ai.params.is_empty() {
            let params_json = params_to_json(&ui.meta.ai.params);
            attr(buf, "data-ai-params", &params_json);
        }
    }
}

fn attr(buf: &mut String, name: &str, value: &str) {
    buf.push(' ');
    buf.push_str(name);
    buf.push_str("=\"");
    buf.push_str(&esc_attr(value));
    buf.push('"');
}

fn has_attrs<Msg>(ui: &UITree<Msg>) -> bool {
    ui.meta.class.is_some()
        || ui.meta.data_appfront_id.is_some()
        || ui.meta.ai.action.is_some()
}

fn params_to_json(pairs: &[(String, String)]) -> String {
    // Build a simple JSON object without pulling in serde_json every time.
    let mut buf = String::from('{');
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        buf.push('"');
        buf.push_str(&esc_json_str(k));
        buf.push_str("\":\"");
        buf.push_str(&esc_json_str(v));
        buf.push('"');
    }
    buf.push('}');
    buf
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

fn esc_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// Escapes a string for safe use inside a double-quoted HTML attribute value.
pub fn esc_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// Escapes a JSON-encoded string for safe embedding inside an inline
/// `<script>` element. `serde_json` does not escape `<`, so a value
/// containing the literal text `</script>` would otherwise terminate the
/// element early and inject attacker-controlled markup; this replaces the
/// HTML-sensitive characters with their `\uXXXX` JSON escapes, which are
/// semantically identical JSON but inert as HTML.
pub fn esc_script_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for ch in json.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c => out.push(c),
        }
    }
    out
}

fn esc_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use appfront_core::ContainerBuilder;

    type Msg = ();

    #[test]
    fn renders_heading() {
        let ui = ui_tree();
        let html = render(&ui);
        assert!(html.contains("<h1 class=\"title\">Dashboard</h1>"), "{html}");
    }

    #[test]
    fn renders_button_with_ai_attrs() {
        let ui = ui_tree();
        let html = render(&ui);
        assert!(html.contains("data-ai-action=\"submit\""), "{html}");
        assert!(html.contains("data-ai-params"), "{html}");
    }

    #[test]
    fn renders_input() {
        let ui = appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.input("hello");
        });
        let html = render(&ui);
        assert!(html.contains(r#"value="hello""#));
    }

    #[test]
    fn renders_full_page() {
        let ui = appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.text("Hello");
        });
        let page = render_page(&ui, "Test Title", "Test Description");
        assert!(page.contains("<!DOCTYPE html>"));
        assert!(page.contains("<title>Test Title</title>"));
        assert!(page.contains(r#"property="og:title""#));
        assert!(page.contains(r#"property="og:description""#));
        assert!(page.contains(">Hello<"));
    }

    #[test]
    fn escapes_special_chars() {
        let ui = appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.text("<script>alert('xss')</script>");
        });
        let html = render(&ui);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn utility_classes_emit_inline_style() {
        let ui = appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Styled").class("bg-blue-500 p-4");
        });
        let html = render(&ui);
        assert!(html.contains(r#"class="bg-blue-500 p-4""#), "{html}");
        assert!(html.contains("background-color: #3b82f6"), "{html}");
        assert!(html.contains("padding: 1rem"), "{html}");
    }

    #[test]
    fn unknown_class_not_styled() {
        let ui = appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.text("plain").class("my-custom-class");
        });
        let html = render(&ui);
        assert!(html.contains(r#"class="my-custom-class""#), "{html}");
        assert!(!html.contains("style="), "{html}");
    }

    #[test]
    fn renders_data_grid() {
        let ui = appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.data_grid(["A", "B"], [vec!["1", "2"]]);
        });
        let html = render(&ui);
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>A</th>"));
        assert!(html.contains("<td>1</td>"));
    }

    fn ui_tree() -> appfront_core::UITree<Msg> {
        appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Dashboard").class("title");
            c.button("Submit")
                .ai_action("submit")
                .ai_param("key", "val");
        })
    }
}
