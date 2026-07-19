//! `UITree` → JSON-LD (schema.org structured data / rich snippets).
//!
//! Produces a `@graph` array suitable for embedding in
//! `<script type="application/ld+json">`. See `docs/ai-schema.md`.

use tpt_appfront_core::{AiMeta, NodeKind, UITree};
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

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_appfront_core::UITree;

    #[derive(Debug, Clone)]
    enum Msg {
        Clicked,
    }

    #[test]
    fn empty_container_has_no_extra_graph_entries() {
        let ui: UITree<Msg> = UITree::container(|_c| {});
        let json = to_json_ld(&ui);
        assert_eq!(json["@context"], "https://schema.org");
        let graph = json["@graph"].as_array().unwrap();
        assert_eq!(graph.len(), 1);
        assert_eq!(graph[0]["@type"], "WebPageElement");
        assert!(graph[0].get("hasPart").is_none());
    }

    #[test]
    fn heading_and_text_populate_expected_fields() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.heading(1, "Title");
            c.text("Body");
        });
        let json = to_json_ld(&ui);
        let graph = json["@graph"].as_array().unwrap();
        // graph[0] is the container itself, then heading, then text.
        assert_eq!(graph[1]["headline"], "Title");
        assert_eq!(graph[2]["text"], "Body");
    }

    #[test]
    fn button_with_ai_action_produces_action_entry() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Add").ai_action("add_to_cart").ai_param("qty", "1");
        });
        let json = to_json_ld(&ui);
        let graph = json["@graph"].as_array().unwrap();
        let entry = &graph[1];
        assert_eq!(entry["@type"], "Action");
        assert_eq!(entry["name"], "Add");
        assert_eq!(entry["target"]["action"], "add_to_cart");
        assert_eq!(entry["target"]["actionParams"]["qty"], "1");
    }

    #[test]
    fn button_with_only_on_click_falls_back_to_click_action() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go").on_click(Msg::Clicked);
        });
        let json = to_json_ld(&ui);
        let entry = &json["@graph"].as_array().unwrap()[1];
        assert_eq!(entry["@type"], "Action");
        assert_eq!(entry["target"]["action"], "click");
    }

    #[test]
    fn button_with_neither_action_nor_click_is_plain_element() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Label");
        });
        let json = to_json_ld(&ui);
        let entry = &json["@graph"].as_array().unwrap()[1];
        assert_eq!(entry["@type"], "WebPageElement");
        assert_eq!(entry["name"], "Label");
    }

    #[test]
    fn input_with_ai_action_has_potential_action() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.input("hello").ai_action("set_value");
        });
        let json = to_json_ld(&ui);
        let entry = &json["@graph"].as_array().unwrap()[1];
        assert_eq!(entry["value"], "hello");
        assert_eq!(entry["potentialAction"]["target"]["action"], "set_value");
    }

    #[test]
    fn list_walks_children_and_reports_item_count() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.list(|l| {
                l.text("one");
                l.text("two");
            });
        });
        let json = to_json_ld(&ui);
        let graph = json["@graph"].as_array().unwrap();
        // graph[0] = outer container, graph[1] = list, then two text children.
        let list_entry = &graph[1];
        assert_eq!(list_entry["itemListElement"].as_array().unwrap().len(), 2);
        assert_eq!(graph[2]["text"], "one");
        assert_eq!(graph[3]["text"], "two");
    }

    #[test]
    fn data_grid_reports_table_shape() {
        let ui: UITree<Msg> =
            UITree::container(|c| {
                c.data_grid(["Name", "Age"], [["Alice", "30"], ["Bob", "25"]]);
            });
        let json = to_json_ld(&ui);
        let entry = &json["@graph"].as_array().unwrap()[1];
        assert_eq!(entry["@type"], "Table");
        assert_eq!(entry["columnList"], serde_json::json!(["Name", "Age"]));
        assert_eq!(
            entry["rows"],
            serde_json::json!([["Alice", "30"], ["Bob", "25"]])
        );
    }
}
