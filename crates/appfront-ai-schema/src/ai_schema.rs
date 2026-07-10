//! `UITree` → custom AI Schema (interactive elements, actions, params).
//!
//! Produces a flat JSON structure optimised for AI agents to understand a
//! page's interactive surface and data content without rendering it.
//! See `docs/ai-schema.md`.

use appfront_core::{NodeKind, UITree};
use serde_json::{Map, Value};

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

    let title = data
        .first()
        .and_then(|d| d.text.clone())
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
        NodeKind::List { items } => {
            for child in items {
                collect(child, interactive, data);
            }
        }
        NodeKind::DataGrid {
            columns,
            rows,
        } => {
            data.push(DataElement {
                kind: "data_grid".to_string(),
                columns: Some(columns.clone()),
                rows: Some(rows.clone()),
                text: None,
            });
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
