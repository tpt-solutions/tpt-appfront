use appfront_core::{view, NodeKind, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
    Submit,
}

fn root_kind<Msg>(ui: &UITree<Msg>) -> &NodeKind<Msg> {
    &ui.kind
}

#[test]
fn builds_a_container_with_all_node_types() {
    let ui = view! {
        <Container>
            <Heading level={1u8}>"Counter"</Heading>
            <Text>{ format!("Count: {}", 3) }</Text>
            <Button on_click={Msg::Increment}>"+1"</Button>
            <Input value={"hi".to_string()} />
        </Container>
    };

    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    assert_eq!(children.len(), 4);

    match &children[0].kind {
        NodeKind::Heading { level, text } => {
            assert_eq!(*level, 1);
            assert_eq!(text, "Counter");
        }
        other => panic!("expected heading, got {other:?}"),
    }
    match &children[1].kind {
        NodeKind::Text { text } => assert_eq!(text, "Count: 3"),
        other => panic!("expected text, got {other:?}"),
    }
    match &children[2].kind {
        NodeKind::Button { label } => {
            assert_eq!(label, "+1");
            assert_eq!(children[2].meta.on_click, Some(Msg::Increment));
        }
        other => panic!("expected button, got {other:?}"),
    }
    match &children[3].kind {
        NodeKind::Input { value } => assert_eq!(value, "hi"),
        other => panic!("expected input, got {other:?}"),
    }
}

#[test]
fn applies_class_and_key_attributes() {
    let ui = view! {
        <Container>
            <Button on_click={Msg::Submit} class={"primary"} key={"submit-btn"}>"Go"</Button>
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    assert_eq!(children[0].meta.class.as_deref(), Some("primary"));
    assert_eq!(children[0].meta.key.as_deref(), Some("submit-btn"));
}

#[test]
fn applies_root_class_attribute() {
    let ui: UITree<Msg> = view! {
        <Container class={"page"}>
            <Text>"hello"</Text>
        </Container>
    };
    assert_eq!(ui.meta.class.as_deref(), Some("page"));
}

#[test]
fn nests_containers() {
    let ui: UITree<Msg> = view! {
        <Container>
            <Container>
                <Text>"inner"</Text>
            </Container>
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    let NodeKind::Container { children: inner } = &children[0].kind else {
        panic!("expected nested container");
    };
    assert_eq!(inner.len(), 1);
    assert!(matches!(inner[0].kind, NodeKind::Text { .. }));
}

#[test]
fn interpolation_expression_text_works() {
    let name = "world";
    let ui: UITree<Msg> = view! {
        <Container>
            <Text>{ format!("hello {name}") }</Text>
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    match &children[0].kind {
        NodeKind::Text { text } => assert_eq!(text, "hello world"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn fully_static_view_is_flagged_static() {
    let ui: UITree<Msg> = view! {
        <Container class={"page"}>
            <Heading level={1u8}>"Title"</Heading>
            <Text>"static body"</Text>
        </Container>
    };
    assert!(!ui.meta.is_dynamic, "static view must set is_dynamic = false");
    assert_eq!(ui.meta.class.as_deref(), Some("page"));
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    assert_eq!(children.len(), 2);
}

#[test]
fn dynamic_view_is_flagged_dynamic() {
    let n = 7;
    let ui: UITree<Msg> = view! {
        <Container>
            <Text>{ format!("n = {n}") }</Text>
        </Container>
    };
    assert!(ui.meta.is_dynamic, "interpolated view must set is_dynamic = true");
}

#[test]
fn static_subtree_inside_dynamic_root_still_builds() {
    // Root is dynamic (interpolation), but the heading child is static and must
    // be hoisted into a cached `UITree` without changing the tree shape.
    let label = "+1";
    let ui: UITree<Msg> = view! {
        <Container>
            <Heading level={1u8}>"Static Heading"</Heading>
            <Button on_click={Msg::Increment}>"label"</Button>  // literal label => static
            <Text>{ format!("dynamic {label}") }</Text>
        </Container>
    };
    assert!(ui.meta.is_dynamic);
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    assert_eq!(children.len(), 3);
    match &children[0].kind {
        NodeKind::Heading { text, .. } => assert_eq!(text, "Static Heading"),
        other => panic!("expected heading, got {other:?}"),
    }
    match &children[1].kind {
        NodeKind::Button { label } => assert_eq!(label, "label"),
        other => panic!("expected button, got {other:?}"),
    }
}

#[test]
fn static_view_is_reproducible_across_calls() {
    // Two separate expansions with identical content must yield equal trees
    // (each caches its own build once). Exercises the static codegen path.
    let a: UITree<Msg> = view! {
        <Container><Text>"same"</Text></Container>
    };
    let b: UITree<Msg> = view! {
        <Container><Text>"same"</Text></Container>
    };
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}
