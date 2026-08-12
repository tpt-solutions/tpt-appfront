//! Semantic HTML (SSR/SSG) backend: `UITree` → semantic HTML string.
//!
//! Produces valid HTML5 fragments or full pages with OpenGraph meta tags
//! and `data-ai-action` / `data-ai-params` attributes for AI crawlers.
//! See `docs/ai-schema.md`.

use tpt_appfront_core::ui_tree::MediaType;
use tpt_appfront_core::{NodeKind, UITree};

/// Renders a `UITree` to a semantic HTML fragment (no `<html>`/`<head>`/`<body>`).
pub fn render<Msg>(ui: &UITree<Msg>) -> String {
    let mut buf = String::new();
    render_node(&mut buf, ui);
    buf
}

/// Renders a full HTML5 page with OpenGraph tags.
pub fn render_page<Msg>(ui: &UITree<Msg>, title: &str, description: &str) -> String {
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
            // Stable `id` so an author-supplied `<label for>` can associate with
            // this control (todo.md #20: label `for=`/`id` wiring).
            if let Some(id) = control_id(ui) {
                attr(buf, "id", &id);
            }
            attr(buf, "value", value);
            buf.push_str(" />");
        }
        NodeKind::Textarea { value } => {
            buf.push_str("<textarea");
            attrs(buf, ui);
            if let Some(id) = control_id(ui) {
                attr(buf, "id", &id);
            }
            buf.push('>');
            buf.push_str(&esc_text(value));
            buf.push_str("</textarea>");
        }
        NodeKind::Checkbox { label, checked } => {
            // Explicit label association (`for`/`id`) in addition to the wrapping
            // `<label>`, so the control is named for assistive tech even if the
            // implicit containment is missed (todo.md #20).
            buf.push_str("<label");
            attrs(buf, ui);
            if let Some(id) = control_id(ui) {
                attr(buf, "for", &id);
            }
            buf.push('>');
            buf.push_str("<input type=\"checkbox\"");
            if let Some(id) = control_id(ui) {
                attr(buf, "id", &id);
            }
            if *checked {
                buf.push_str(" checked");
            }
            // Explicit ARIA state so screen readers announce the toggle even
            // when the native checkbox semantics are overridden (todo.md #16).
            buf.push_str(" aria-checked=\"");
            buf.push_str(if *checked { "true" } else { "false" });
            buf.push('"');
            if !label.is_empty() {
                attr(buf, "aria-label", label);
            }
            buf.push_str(" /> ");
            buf.push_str(&esc_text(label));
            close_tag(buf, "label");
        }
        NodeKind::Select { options, selected } => {
            buf.push_str("<select");
            attrs(buf, ui);
            // A native `<select>` already exposes the correct implicit role, so
            // the remaining gap is an accessible name when there's no associated
            // `<label>`; surface `meta.ai.description` as `aria-label` if set
            // (todo.md #20: Select ARIA).
            if let Some(id) = control_id(ui) {
                attr(buf, "id", &id);
            }
            if let Some(desc) = &ui.meta.ai.description {
                if !desc.is_empty() {
                    attr(buf, "aria-label", desc);
                }
            }
            buf.push('>');
            for (value, label) in options {
                buf.push_str("<option value=\"");
                buf.push_str(&esc_attr(value));
                buf.push('"');
                if value == selected {
                    buf.push_str(" selected");
                }
                buf.push('>');
                buf.push_str(&esc_text(label));
                buf.push_str("</option>");
            }
            close_tag(buf, "select");
        }
        NodeKind::Radio {
            name,
            options,
            selected,
        } => {
            // `role="radiogroup"` groups the options for assistive tech; each
            // option carries its own `aria-checked`/`aria-label` and an explicit
            // `for`/`id` label association (todo.md #16 + #20).
            buf.push_str("<div");
            attrs(buf, ui);
            buf.push_str(" role=\"radiogroup\">");
            for (i, (value, label)) in options.iter().enumerate() {
                let opt_id = control_id(ui)
                    .map(|g| format!("{g}-{i}"))
                    .unwrap_or_else(|| format!("af-opt-{i}"));
                buf.push_str("<label");
                attr(buf, "for", &opt_id);
                buf.push('>');
                buf.push_str("<input type=\"radio\" name=\"");
                buf.push_str(&esc_attr(name));
                buf.push_str("\" value=\"");
                buf.push_str(&esc_attr(value));
                buf.push('"');
                attr(buf, "id", &opt_id);
                let is_selected = value == selected;
                if is_selected {
                    buf.push_str(" checked");
                }
                buf.push_str(" aria-checked=\"");
                buf.push_str(if is_selected { "true" } else { "false" });
                buf.push('"');
                if !label.is_empty() {
                    attr(buf, "aria-label", label);
                }
                buf.push_str(" /> ");
                buf.push_str(&esc_text(label));
                buf.push_str("</label>");
            }
            close_tag(buf, "div");
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
            // `role="grid"` plus `role="row"`/`role="columnheader"`/`role="gridcell"`
            // make the table a proper ARIA grid (sortable/focusable cells)
            // instead of relying solely on the implicit `<table>` semantics
            // (todo.md #20).
            buf.push_str("<table");
            attrs(buf, ui);
            attr(buf, "role", "grid");
            buf.push('>');

            // <thead>
            buf.push_str("<thead><tr role=\"row\">");
            for col in columns {
                // `scope="col"` tells screen readers each header describes its
                // column (todo.md #16).
                buf.push_str("<th scope=\"col\" role=\"columnheader\">");
                buf.push_str(&esc_text(col));
                buf.push_str("</th>");
            }
            buf.push_str("</tr></thead>");

            // <tbody>
            buf.push_str("<tbody>");
            for row in rows {
                buf.push_str("<tr role=\"row\">");
                for cell in row {
                    buf.push_str("<td role=\"gridcell\">");
                    buf.push_str(&esc_text(cell));
                    buf.push_str("</td>");
                }
                buf.push_str("</tr>");
            }
            buf.push_str("</tbody>");

            close_tag(buf, "table");
        }
        NodeKind::Image { src, alt } => {
            buf.push_str("<img");
            attrs(buf, ui);
            attr(buf, "src", src);
            attr(buf, "alt", alt);
            buf.push_str(" />");
        }
        NodeKind::Link { href, text } => {
            buf.push_str("<a");
            attrs(buf, ui);
            attr(buf, "href", href);
            buf.push('>');
            buf.push_str(&esc_text(text));
            close_tag(buf, "a");
        }
        NodeKind::Media { src, alt, media_type } => {
            let tag = match media_type {
                MediaType::Audio => "audio",
                MediaType::Video => "video",
            };
            buf.push_str(tag);
            attrs(buf, ui);
            attr(buf, "src", src);
            buf.push_str(" controls");
            if !alt.is_empty() {
                attr(buf, "aria-label", alt);
            }
            buf.push('>');
            close_tag(buf, tag);
        }
        NodeKind::Portal { target, content } => {
            // Render the portal content inline but tag it so a crawler/host can
            // find and relocate it; `UITree::collect_portals` is the
            // authoritative extraction path for SSR hosts.
            buf.push_str("<div data-portal-target=\"");
            buf.push_str(&esc_text(target));
            buf.push_str("\">");
            render_node(buf, content);
            buf.push_str("</div>");
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

/// Returns a stable DOM `id` for a form control, derived from the node's
/// assigned `data_appfront_id` (set during SSR via [`UITree::assign_ids`]).
/// `None` when the node hasn't been assigned an id yet (e.g. pure client-side
/// renders that never call `assign_ids`), in which case no `id` is emitted.
fn control_id<Msg>(ui: &UITree<Msg>) -> Option<String> {
    ui.meta.data_appfront_id.map(|id| format!("af-{id}"))
}

fn attrs<Msg>(buf: &mut String, ui: &UITree<Msg>) {
    if let Some(class) = &ui.meta.class {
        attr(buf, "class", class);
        // Tailwind-style utility layer: recognized utility classes (e.g.
        // `bg-blue-500 p-4`) are resolved to real CSS so SSR output is
        // actually styled without a separate build step. See
        // `tpt_appfront_core::styling`.
        let style = tpt_appfront_core::styling::inline_style(class);
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
    ui.meta.class.is_some() || ui.meta.data_appfront_id.is_some() || ui.meta.ai.action.is_some()
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
    use tpt_appfront_core::ContainerBuilder;

    use super::*;

    type Msg = ();

    #[test]
    fn renders_heading() {
        let ui = ui_tree();
        let html = render(&ui);
        assert!(
            html.contains("<h1 class=\"title\">Dashboard</h1>"),
            "{html}"
        );
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
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.input("hello");
        });
        let html = render(&ui);
        assert!(html.contains(r#"value="hello""#));
    }

    #[test]
    fn renders_full_page() {
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
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
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.text("<script>alert('xss')</script>");
        });
        let html = render(&ui);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn utility_classes_emit_inline_style() {
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Styled").class("bg-blue-500 p-4");
        });
        let html = render(&ui);
        assert!(html.contains(r#"class="bg-blue-500 p-4""#), "{html}");
        assert!(html.contains("background-color: #3b82f6"), "{html}");
        assert!(html.contains("padding: 1rem"), "{html}");
    }

    #[test]
    fn unknown_class_not_styled() {
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.text("plain").class("my-custom-class");
        });
        let html = render(&ui);
        assert!(html.contains(r#"class="my-custom-class""#), "{html}");
        assert!(!html.contains("style="), "{html}");
    }

    #[test]
    fn renders_data_grid() {
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.data_grid(["A", "B"], [vec!["1", "2"]]);
        });
        let html = render(&ui);
        assert!(html.contains("<table role=\"grid\">"));
        assert!(html.contains("<th scope=\"col\" role=\"columnheader\">A</th>"));
        assert!(html.contains("<td role=\"gridcell\">1</td>"));
    }

    #[test]
    fn renders_aria_attributes_for_form_controls() {
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.checkbox("Agree", true);
            c.radio_group("color", [("r", "Red"), ("g", "Green")], "g");
            c.data_grid(["A", "B"], [vec!["1", "2"]]);
        });
        let html = render(&ui);
        assert!(html.contains("type=\"checkbox\" checked aria-checked=\"true\""), "{html}");
        assert!(html.contains("aria-label=\"Agree\""), "{html}");
        assert!(html.contains("role=\"radiogroup\""), "{html}");
        assert!(html.contains("aria-checked=\"true\""), "{html}");
        assert!(html.contains("aria-label=\"Green\""), "{html}");
        assert!(html.contains("<th scope=\"col\" role=\"columnheader\">A</th>"), "{html}");
    }

    #[test]
    fn renders_form_control_label_association_and_select_aria() {
        // Assign ids first, so the `for=`/`id` wiring has something to point at.
        let mut ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.checkbox("Agree", true);
            c.radio_group("color", [("r", "Red"), ("g", "Green")], "g");
            c.select([("a", "Alpha"), ("b", "Beta")], "b")
                .ai_description("Pick a letter");
        });
        ui.assign_ids();

        let html = render(&ui);
        // Checkbox: control `id` + explicit `for` on the wrapping label.
        assert!(html.contains("id=\"af-2\""), "{html}");
        assert!(html.contains("for=\"af-2\""), "{html}");
        // Radio: each option gets its own `id`/`for` derived from the group id.
        assert!(html.contains("id=\"af-3-0\""), "{html}");
        assert!(html.contains("for=\"af-3-0\""), "{html}");
        assert!(html.contains("id=\"af-3-1\""), "{html}");
        assert!(html.contains("for=\"af-3-1\""), "{html}");
        // Select: native role is implicit, but the accessible name is surfaced
        // as `aria-label` from `meta.ai.description`.
        assert!(html.contains("aria-label=\"Pick a letter\""), "{html}");
    }

    #[test]
    fn renders_form_controls() {
        let ui = tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.textarea("notes");
            c.checkbox("Agree", true);
            c.select([("a", "Alpha"), ("b", "Beta")], "b");
            c.radio_group("color", [("r", "Red"), ("g", "Green")], "g");
        });
        let html = render(&ui);
        assert!(html.contains("<textarea"), "{html}");
        assert!(html.contains(">notes</textarea>"), "{html}");
        assert!(html.contains("type=\"checkbox\" checked"), "{html}");
        assert!(
            html.contains("<option value=\"b\" selected>Beta</option>"),
            "{html}"
        );
        assert!(html.contains("name=\"color\""), "{html}");
        assert!(html.contains("value=\"g\""), "{html}");
        assert!(html.contains("checked"), "{html}");
    }

    fn ui_tree() -> tpt_appfront_core::UITree<Msg> {
        tpt_appfront_core::UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Dashboard").class("title");
            c.button("Submit")
                .ai_action("submit")
                .ai_param("key", "val");
        })
    }
}
