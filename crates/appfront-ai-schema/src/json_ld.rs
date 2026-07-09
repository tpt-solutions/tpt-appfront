//! `UITree` → JSON-LD (schema.org structured data / rich snippets).
//!
//! Produces a `@graph` array suitable for embedding in
//! `<script type="application/ld+json">`. See `docs/ai-schema.md`.

use appfront_core::{AiMeta, NodeKind, UITree};
use serde_json::{Map, Value};

/// Serialises `ui` as a JSON-LD `@graph` array.
pub fn to_json_ld<Msg>(ui: &UITree<Msg>) -> Value {
    let mut graph: Vec<Value> = Vec::new();
    walk(ui, &mut graph);
    serde_json::json!({
        "@context": "https://schema.org",
        "@graph": graph,
    })
}

fn walk<Msg>(ui: &UITree<Msg>, graph: &mut Vec<Value>) {
    match &ui.kind {
        NodeKind::Container { children } => {
            let mut item = web_page_element(&ui.meta.ai);
            let parts: Vec<Value> = children.iter().map(|_| Value::Null).collect();
            if !parts.is_empty() {
                item.insert("hasPart".to_string(), Value::Array(parts));
            }
            graph.push(Value::Object(item));
            for child in children {
                walk(child, graph);
            }
        }
        NodeKind::Heading { text, .. } => {
            let mut item = web_page_element(&ui.meta.ai);
            item.insert("headline".to_string(), Value::String(text.clone()));
            graph.push(Value::Object(item));
        }
        NodeKind::Text { text } => {
            let mut item = web_page_element(&ui.meta.ai);
            item.insert("text".to_string(), Value::String(text.clone()));
            graph.push(Value::Object(item));
        }
        NodeKind::Button { label } => {
            if let Some(action) = &ui.meta.ai.action {
                graph.push(action_entry(label, action, &ui.meta.ai.params));
            } else if ui.meta.on_click.is_some() {
                graph.push(action_entry(label, "click", &[]));
            } else {
                let mut item = web_page_element(&ui.meta.ai);
                item.insert("name".to_string(), Value::String(label.clone()));
                graph.push(Value::Object(item));
            }
        }
        NodeKind::Input { value } => {
            let mut item = web_page_element(&ui.meta.ai);
            item.insert("name".to_string(), Value::String("input".to_string()));
            item.insert("value".to_string(), Value::String(value.clone()));
            if let Some(action) = &ui.meta.ai.action {
                item.insert(
                    "potentialAction".to_string(),
                    action_entry(&format!("Set {}", value), action, &ui.meta.ai.params),
                );
            }
            graph.push(Value::Object(item));
        }
        NodeKind::List { items } => {
            let mut item = web_page_element(&ui.meta.ai);
            let elements: Vec<Value> = items
                .iter()
                .map(|_| {
                    serde_json::json!({
                        "@type": "ListItem",
                        "position": 0
                    })
                })
                .collect();
            item.insert("itemListElement".to_string(), Value::Array(elements));
            graph.push(Value::Object(item));
            for child in items {
                walk(child, graph);
            }
        }
        NodeKind::DataGrid { columns, rows } => {
            let mut item = web_page_element(&ui.meta.ai);
            item.insert("@type".to_string(), Value::String("Table".to_string()));
            item.insert("about".to_string(), Value::String(columns.join(", ")));
            item.insert(
                "columnList".to_string(),
                Value::Array(
                    columns.iter().map(|c| Value::String(c.clone())).collect(),
                ),
            );
            let row_values: Vec<Value> = rows
                .iter()
                .map(|r| {
                    Value::Array(r.iter().map(|c| Value::String(c.clone())).collect())
                })
                .collect();
            item.insert("rows".to_string(), Value::Array(row_values));
            graph.push(Value::Object(item));
        }
    }
}

fn web_page_element(ai: &AiMeta) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("@type".to_string(), Value::String("WebPageElement".to_string()));
    if let Some(desc) = &ai.description {
        m.insert("description".to_string(), Value::String(desc.clone()));
    }
    m
}

fn action_entry(name: &str, action: &str, params: &[(String, String)]) -> Value {
    let mut target = Map::new();
    target.insert(
        "@type".to_string(),
        Value::String("EntryPoint".to_string()),
    );
    target.insert("action".to_string(), Value::String(action.to_string()));

    if !params.is_empty() {
        let p: Map<String, Value> = params
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        target.insert("actionParams".to_string(), Value::Object(p));
    }

    serde_json::json!({
        "@type": "Action",
        "name": name,
        "target": target,
    })
}
