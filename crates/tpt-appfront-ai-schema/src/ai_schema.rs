//! `UITree` → custom AI Schema (interactive elements, actions, params).
//!
//! Produces a flat JSON structure optimised for AI agents to understand a
//! page's interactive surface and data content without rendering it.
//! See `docs/ai-schema.md`.

use serde_json::{Map, Value};
use tpt_appfront_core::ui_tree::MediaType;
use tpt_appfront_core::{NodeKind, UITree};

/// Describes the interactive surface of a `UITree` for AI agents.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiSchemaOutput {
    pub schema_version: String,
    pub title: String,
    #[serde(default)]
    pub interactive: Vec<InteractiveElement>,
    #[serde(default)]
    pub data: Vec<DataElement>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InteractiveElement {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: Option<String>,
    pub value: Option<String>,
    pub action: Option<String>,
    #[serde(default)]
    pub params: Map<String, Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DataElement {
    #[serde(rename = "type")]
    pub kind: String,
    pub columns: Option<Vec<String>>,
    pub rows: Option<Vec<Vec<String>>>,
    pub text: Option<String>,
}

/// Builds an [`AiSchemaOutput`] from a `UITree`.
pub fn to_ai_schema<Msg>(ui: &UITree<Msg>) -> AiSchemaOutput {
    let mut interactive = Vec::new();
    let mut data = Vec::new();
    collect(ui, &mut interactive, &mut data);

    // An explicit title on the root node's `AiMeta` overrides the heuristic so a
    // breadcrumb/aside rendering before the real heading can't mis-name the
    // page. The heuristic only runs when no explicit title is set.
    let title = ui
        .meta
        .ai
        .title
        .clone()
        .or_else(|| data.first().and_then(|d| d.text.clone()))
        .unwrap_or_default();

    AiSchemaOutput {
        schema_version: "0.1.0".to_string(),
        title,
        interactive,
        data,
    }
}

/// Serialises the schema directly to a JSON `Value`. Returns an error
/// instead of panicking if serialisation ever fails (e.g. a future `Msg`
/// type whose `Serialize` impl can fail), since this runs on every
/// AI-agent request.
pub fn to_ai_schema_value<Msg>(ui: &UITree<Msg>) -> Result<Value, serde_json::Error> {
    let schema = to_ai_schema(ui);
    serde_json::to_value(schema)
}

fn collect<Msg>(
    ui: &UITree<Msg>,
    interactive: &mut Vec<InteractiveElement>,
    data: &mut Vec<DataElement>,
) {
    match &ui.kind {
        NodeKind::Container { children } => {
            for child in children {
                collect(child, interactive, data);
            }
        }
        NodeKind::Heading { text, .. } => {
            data.push(DataElement {
                kind: "heading".to_string(),
                columns: None,
                rows: None,
                text: Some(text.clone()),
            });
        }
        NodeKind::Text { text } => {
            data.push(DataElement {
                kind: "text".to_string(),
                columns: None,
                rows: None,
                text: Some(text.clone()),
            });
        }
        NodeKind::Button { label } => {
            let params = build_params(&ui.meta.ai.params);
            interactive.push(InteractiveElement {
                kind: "button".to_string(),
                label: Some(label.clone()),
                value: None,
                action: ui.meta.ai.action.clone(),
                params,
            });
        }
        NodeKind::Input { value } => {
            let params = build_params(&ui.meta.ai.params);
            interactive.push(InteractiveElement {
                kind: "input".to_string(),
                label: None,
                value: Some(value.clone()),
                action: ui.meta.ai.action.clone(),
                params,
            });
        }
        NodeKind::Textarea { value } => {
            let params = build_params(&ui.meta.ai.params);
            interactive.push(InteractiveElement {
                kind: "textarea".to_string(),
                label: None,
                value: Some(value.clone()),
                action: ui.meta.ai.action.clone(),
                params,
            });
        }
        NodeKind::Checkbox { label, checked } => {
            let params = build_params(&ui.meta.ai.params);
            interactive.push(InteractiveElement {
                kind: "checkbox".to_string(),
                label: Some(label.clone()),
                value: Some(checked.to_string()),
                action: ui.meta.ai.action.clone(),
                params,
            });
        }
        NodeKind::Select { selected, .. } => {
            let params = build_params(&ui.meta.ai.params);
            interactive.push(InteractiveElement {
                kind: "select".to_string(),
                label: None,
                value: Some(selected.clone()),
                action: ui.meta.ai.action.clone(),
                params,
            });
        }
        NodeKind::Radio { selected, .. } => {
            let params = build_params(&ui.meta.ai.params);
            interactive.push(InteractiveElement {
                kind: "radio".to_string(),
                label: None,
                value: Some(selected.clone()),
                action: ui.meta.ai.action.clone(),
                params,
            });
        }
        NodeKind::List { items } => {
            for child in items {
                collect(child, interactive, data);
            }
        }
        NodeKind::DataGrid { columns, rows } => {
            data.push(DataElement {
                kind: "data_grid".to_string(),
                columns: Some(columns.clone()),
                rows: Some(rows.clone()),
                text: None,
            });
        }
        NodeKind::Image { alt, .. } => {
            data.push(DataElement {
                kind: "image".to_string(),
                columns: None,
                rows: None,
                text: Some(alt.clone()),
            });
        }
        NodeKind::Link { text, .. } => {
            data.push(DataElement {
                kind: "link".to_string(),
                columns: None,
                rows: None,
                text: Some(text.clone()),
            });
        }
        NodeKind::Media {
            alt, media_type, ..
        } => {
            data.push(DataElement {
                kind: match media_type {
                    MediaType::Audio => "audio",
                    MediaType::Video => "video",
                }
                .to_string(),
                columns: None,
                rows: None,
                text: Some(alt.clone()),
            });
        }
        NodeKind::Viewport { alt, .. } => {
            data.push(DataElement {
                kind: "viewport".to_string(),
                columns: None,
                rows: None,
                text: Some(alt.clone()),
            });
        }
        NodeKind::Portal { content, .. } => {
            // Surface the portal's content so AI consumers see its elements.
            collect(content, interactive, data);
        }
    }
}

fn build_params(pairs: &[(String, String)]) -> Map<String, Value> {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k.clone(), Value::String(v.clone()));
    }
    map
}

#[cfg(test)]
mod tests {
    use tpt_appfront_core::UITree;

    use super::*;

    #[derive(Debug, Clone)]
    enum Msg {
        Clicked,
    }

    #[test]
    fn title_comes_from_first_data_element() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.heading(1, "Welcome");
            c.button("Go").on_click(Msg::Clicked);
        });
        let schema = to_ai_schema(&ui);
        assert_eq!(schema.title, "Welcome");
        assert_eq!(schema.interactive.len(), 1);
        assert_eq!(schema.interactive[0].kind, "button");
        assert_eq!(schema.interactive[0].label.as_deref(), Some("Go"));
    }

    #[test]
    fn button_and_input_params_round_trip() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Add")
                .ai_action("add_to_cart")
                .ai_param("qty", "2");
            c.input("hi").ai_param("max_len", "10");
        });
        let schema = to_ai_schema(&ui);
        assert_eq!(schema.interactive.len(), 2);
        assert_eq!(schema.interactive[0].action.as_deref(), Some("add_to_cart"));
        assert_eq!(
            schema.interactive[0].params.get("qty").unwrap(),
            &Value::String("2".to_string())
        );
        assert_eq!(schema.interactive[1].value.as_deref(), Some("hi"));
        assert_eq!(
            schema.interactive[1].params.get("max_len").unwrap(),
            &Value::String("10".to_string())
        );
    }

    #[test]
    fn list_and_nested_container_recurse() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.container(|inner| {
                inner.button("Nested").on_click(Msg::Clicked);
            });
            c.list(|l| {
                l.text("item one");
                l.text("item two");
            });
        });
        let schema = to_ai_schema(&ui);
        assert_eq!(schema.interactive.len(), 1);
        assert_eq!(schema.interactive[0].label.as_deref(), Some("Nested"));
        assert_eq!(schema.data.len(), 2);
        assert_eq!(schema.data[0].text.as_deref(), Some("item one"));
        assert_eq!(schema.data[1].text.as_deref(), Some("item two"));
    }

    #[test]
    fn data_grid_produces_data_grid_element() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.data_grid(["Name", "Age"], [["Alice", "30"]]);
        });
        let schema = to_ai_schema(&ui);
        assert_eq!(schema.data.len(), 1);
        assert_eq!(schema.data[0].kind, "data_grid");
        assert_eq!(
            schema.data[0].columns.as_deref(),
            Some(&["Name".to_string(), "Age".to_string()][..])
        );
        assert_eq!(
            schema.data[0].rows.as_deref(),
            Some(&[vec!["Alice".to_string(), "30".to_string()]][..])
        );
    }

    #[test]
    fn to_ai_schema_value_serialises_expected_shape() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.heading(1, "Hi");
            c.button("Go").on_click(Msg::Clicked);
        });
        let value = to_ai_schema_value(&ui).unwrap();
        assert_eq!(value["schema_version"], "0.1.0");
        assert_eq!(value["interactive"][0]["type"], "button");
    }
}

/// Renders the AI schema as an `llms.txt`-style Markdown page: the title, the page's
/// content, and every interactive element with its action and params. Meant for an
/// `/llms.txt` route, so an agent can read the page without running it.
pub fn to_llms_txt<Msg>(ui: &UITree<Msg>) -> String {
    let schema = to_ai_schema(ui);
    let mut out = String::new();
    let title = if schema.title.is_empty() {
        "Untitled page"
    } else {
        schema.title.as_str()
    };
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(&format!(
        "> Machine-readable summary (AI schema {}).\n\n",
        schema.schema_version
    ));

    if !schema.data.is_empty() {
        out.push_str("## Content\n\n");
        for d in &schema.data {
            match (&d.columns, &d.rows, &d.text) {
                (Some(columns), Some(rows), _) => {
                    out.push_str(&format!(
                        "- {} ({} rows): columns {}\n",
                        d.kind,
                        rows.len(),
                        columns.join(", ")
                    ));
                }
                (_, _, Some(text)) => out.push_str(&format!("- {}: {}\n", d.kind, text)),
                _ => out.push_str(&format!("- {}\n", d.kind)),
            }
        }
        out.push('\n');
    }

    if !schema.interactive.is_empty() {
        out.push_str("## Interactive elements\n\n");
        for el in &schema.interactive {
            let name = el.label.as_deref().or(el.value.as_deref()).unwrap_or("");
            let action = el
                .action
                .as_deref()
                .map(|a| format!(", action `{a}`"))
                .unwrap_or_default();
            let params = if el.params.is_empty() {
                String::new()
            } else {
                let names: Vec<&str> = el.params.keys().map(String::as_str).collect();
                format!(", params {}", names.join(", "))
            };
            out.push_str(&format!("- {} \"{}\"{}{}\n", el.kind, name, action, params));
        }
    }
    out
}

/// Describes the page's actions as an OpenAPI 3.1 document for the smart router's
/// `POST /command` endpoint. The `action` field is an enum of the actions this page
/// exposes, so a generated client can only call what the page offers.
pub fn to_openapi<Msg>(ui: &UITree<Msg>) -> Value {
    let schema = to_ai_schema(ui);
    let mut actions: Vec<String> = Vec::new();
    for el in &schema.interactive {
        if let Some(a) = &el.action {
            if !actions.contains(a) {
                actions.push(a.clone());
            }
        }
    }
    let title = if schema.title.is_empty() {
        "TPT AppFront page"
    } else {
        schema.title.as_str()
    };
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": title,
            "version": schema.schema_version,
            "description": "Actions exposed by this page's AI schema."
        },
        "paths": {
            "/command": {
                "post": {
                    "summary": "Run one of the page's actions",
                    "operationId": "runCommand",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["action"],
                                    "properties": {
                                        "action": { "type": "string", "enum": actions },
                                        "params": { "type": "object", "additionalProperties": true }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "The action ran." },
                        "403": { "description": "The action is not allowed." },
                        "429": { "description": "Rate limit exceeded." }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod export_tests {
    use tpt_appfront_core::ContainerBuilder;

    use super::*;

    fn page() -> UITree<()> {
        UITree::container(|c: &mut ContainerBuilder<()>| {
            c.heading(1, "Orders");
            c.button("Export").on_click(()).ai_action("export_data");
            c.input("").ai_action("set_filter");
        })
    }

    #[test]
    fn llms_txt_lists_content_and_actions() {
        let text = to_llms_txt(&page());
        assert!(text.starts_with("# Orders\n"), "{text}");
        assert!(text.contains("- heading: Orders"), "{text}");
        assert!(
            text.contains("- button \"Export\", action `export_data`"),
            "{text}"
        );
        assert!(text.contains("- input \"\", action `set_filter`"), "{text}");
    }

    #[test]
    fn openapi_enumerates_only_this_pages_actions() {
        let doc = to_openapi(&page());
        assert_eq!(doc["openapi"], "3.1.0");
        let action = &doc["paths"]["/command"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"]["properties"]["action"];
        assert_eq!(
            action["enum"],
            serde_json::json!(["export_data", "set_filter"])
        );
    }
}
