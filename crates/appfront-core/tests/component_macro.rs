use appfront_core::{component, ContainerBuilder, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Clicked,
}

/// Renders a labelled counter button.
#[component]
fn counter_row(label: &str) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.button(label).on_click(Msg::Clicked);
    })
}

#[component]
fn tagged_row() -> UITree<Msg> {
    let mut ui = UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.text("hi");
    });
    ui.meta.class = Some("custom-class".to_string());
    ui
}

#[test]
fn component_defaults_class_to_kebab_case_fn_name() {
    let ui = counter_row("+1");
    assert_eq!(ui.meta.class.as_deref(), Some("counter-row"));
}

#[test]
fn component_pulls_doc_comment_into_ai_description() {
    let ui = counter_row("+1");
    assert_eq!(
        ui.meta.ai.description.as_deref(),
        Some("Renders a labelled counter button.")
    );
}

#[test]
fn component_does_not_override_explicit_class() {
    let ui = tagged_row();
    assert_eq!(ui.meta.class.as_deref(), Some("custom-class"));
}
