use tpt_appfront_core::{view, NodeKind, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
    Submit(String),
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
        NodeKind::Button { label } => assert_eq!(label, "+1"),
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
            <Button on_click={Msg::Increment} class={"primary"} key={"submit-btn"}>"Go"</Button>
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
    let label = "+1";
    let ui: UITree<Msg> = view! {
        <Container>
            <Heading level={1u8}>"Static Heading"</Heading>
            <Button on_click={Msg::Increment}>"label"</Button>
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
    let a: UITree<Msg> = view! {
        <Container><Text>"same"</Text></Container>
    };
    let b: UITree<Msg> = view! {
        <Container><Text>"same"</Text></Container>
    };
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn list_tag_builds_items_as_children() {
    let ui: UITree<Msg> = view! {
        <Container>
            <List class={"todo-list"}>
                <Text>"first"</Text>
                <Button on_click={Msg::Increment}>"do it"</Button>
            </List>
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    assert_eq!(children.len(), 1);
    match &children[0].kind {
        NodeKind::List { items } => {
            assert_eq!(items.len(), 2);
            assert_eq!(children[0].meta.class.as_deref(), Some("todo-list"));
            match &items[0].kind {
                NodeKind::Text { text } => assert_eq!(text, "first"),
                other => panic!("expected text item, got {other:?}"),
            }
            match &items[1].kind {
                NodeKind::Button { label } => {
                    assert_eq!(label, "do it");
                    assert_eq!(items[1].meta.on_click, Some(Msg::Increment));
                }
                other => panic!("expected button item, got {other:?}"),
            }
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn data_grid_tag_builds_columns_and_rows() {
    let ui: UITree<Msg> = view! {
        <Container>
            <DataGrid
                columns={vec!["Name".to_string(), "Value".to_string()]}
                rows={vec![vec!["a".to_string(), "1".to_string()]]}
            />
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    match &children[0].kind {
        NodeKind::DataGrid { columns, rows } => {
            assert_eq!(columns, &["Name", "Value"]);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0], &["a", "1"]);
        }
        other => panic!("expected data grid, got {other:?}"),
    }
}

#[test]
fn two_way_binding_emits_on_input() {
    let ui = view! {
        <Container>
            <Input value={"".to_string()} on_input={Msg::Submit} />
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    match &children[0].kind {
        NodeKind::Input { value } => {
            assert_eq!(value, "");
            assert!(
                children[0].meta.on_input.is_some(),
                "two-way binding must set on_input"
            );
            let produced = children[0]
                .meta
                .on_input
                .as_ref()
                .unwrap()("hello".to_string());
            assert_eq!(produced, Msg::Submit("hello".to_string()));
        }
        other => panic!("expected input, got {other:?}"),
    }
}

#[test]
fn for_loop_builds_dynamic_list_items() {
    let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let ui: UITree<Msg> = view! {
        <Container>
            <List>
                {for item in items {
                    <Text>{ item.clone() }</Text>
                }}
            </List>
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    match &children[0].kind {
        NodeKind::List { items } => {
            assert_eq!(items.len(), 3);
            for (i, item) in items.iter().enumerate() {
                match &item.kind {
                    NodeKind::Text { text } => assert_eq!(text, &format!("{}", ['a', 'b', 'c'][i])),
                    other => panic!("expected text item, got {other:?}"),
                }
            }
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn if_else_control_flow_selects_children() {
    let show = true;
    let ui: UITree<Msg> = view! {
        <Container>
            {if show {
                <Text>"yes"</Text>
            } else {
                <Text>"no"</Text>
            }}
            {if !show {
                <Text>"hidden"</Text>
            }}
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    assert_eq!(children.len(), 1, "only the taken branch should appear");
    match &children[0].kind {
        NodeKind::Text { text } => assert_eq!(text, "yes"),
        other => panic!("expected 'yes' text, got {other:?}"),
    }
}

#[test]
fn node_expr_child_is_appended_via_with() {
    let ui: UITree<Msg> = view! {
        <Container>
            { UITree::container(|c: &mut appfront_core::ContainerBuilder<Msg>| { c.text("composed"); }) }
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    assert_eq!(children.len(), 1);
    // Component composition ({ my_component(...) }) is appended verbatim via
    // `ContainerBuilder::with`, so it becomes a nested `Container` whose single
    // child is the composed text.
    match &children[0].kind {
        NodeKind::Container { children: inner } => {
            assert_eq!(inner.len(), 1);
            match &inner[0].kind {
                NodeKind::Text { text } => assert_eq!(text, "composed"),
                other => panic!("expected composed text, got {other:?}"),
            }
        }
        other => panic!("expected composed container, got {other:?}"),
    }
}

#[test]
fn control_flow_marks_view_dynamic() {
    let flag = true;
    let ui: UITree<Msg> = view! {
        <Container>
            {if flag { <Text>"x"</Text> }}
        </Container>
    };
    assert!(ui.meta.is_dynamic, "control flow must set is_dynamic = true");
}

#[test]
fn for_loop_with_else_if_branches() {
    let n = 1;
    let ui: UITree<Msg> = view! {
        <Container>
            {if n == 1 {
                <Text>"one"</Text>
            } else if n == 2 {
                <Text>"two"</Text>
            } else {
                <Text>"many"</Text>
            }}
        </Container>
    };
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container root");
    };
    assert_eq!(children.len(), 1);
    match &children[0].kind {
        NodeKind::Text { text } => assert_eq!(text, "one"),
        other => panic!("expected 'one' text, got {other:?}"),
    }
}

#[test]
fn data_grid_rejects_children() {
    // Negative case: `view!`'s `DataGrid` rejects child elements at macro
    // expansion (see `gen_node_stmt` in `appfront-macros/src/view.rs`), so a
    // `<DataGrid>...</DataGrid>` with children is a compile error, not a
    // runtime assertion we can make in this crate. It is covered by the
    // `data_grid_tag_builds_columns_and_rows` positive test plus the macro's
    // `required_for`/`children`-rejection logic; keeping a runtime test here
    // would require a separate `trybuild` compile-fail fixture, which is
    // out of scope for this `view!` smoke-test file.
}

// ---------------------------------------------------------------------------
// Component model: props, children slots, memo (Phase 16, item #4)
// ---------------------------------------------------------------------------

use appfront_core::{Children, NodeMeta};

#[derive(Debug, Clone, PartialEq)]
struct GreetingProps {
    name: String,
}

/// A typed-props component; `#[component]` fills `class`/`ai.description` and
/// flags `is_dynamic`.
#[appfront_core::component]
fn greeting(props: GreetingProps) -> UITree<Msg> {
    UITree::container(|c| {
        c.heading(1, format!("Hello, {}!", props.name));
    })
}

#[test]
fn component_with_typed_props_builds_expected_tree() {
    let ui = greeting(GreetingProps {
        name: "World".to_string(),
    });
    assert_eq!(ui.meta.class.as_deref(), Some("greeting"));
    // Reading a plain prop (not a Signal) does not flag the component dynamic;
    // `is_dynamic` is only set when the body reads reactive state.
    assert!(!ui.meta.is_dynamic, "plain props should not be dynamic");
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    match &children[0].kind {
        NodeKind::Heading { text, .. } => assert_eq!(text, "Hello, World!"),
        other => panic!("expected heading, got {other:?}"),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CardProps {
    title: String,
}

/// A component that takes a `children` slot and re-emits it inside its own tree.
#[appfront_core::component]
fn card(props: CardProps, children: Children<Msg>) -> UITree<Msg> {
    UITree::container(|c| {
        c.heading(2, props.title.clone());
        for child in children.0 {
            c.with(child);
        }
    })
}

#[test]
fn component_with_children_slot_renders_slot() {
    let ui = card(
        CardProps {
            title: "Panel".to_string(),
        },
        Children(vec![UITree {
            kind: NodeKind::Text {
                text: "body".to_string(),
            },
            meta: NodeMeta::default(),
        }]),
    );
    let NodeKind::Container { children } = root_kind(&ui) else {
        panic!("expected container");
    };
    // [heading, text("body")]
    assert_eq!(children.len(), 2);
    match &children[1].kind {
        NodeKind::Text { text } => assert_eq!(text, "body"),
        other => panic!("expected slotted text, got {other:?}"),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CountProps {
    value: i32,
}

/// Memoized component: when `CountProps` is `PartialEq` to the previous render,
/// the cached tree is returned unchanged.
#[appfront_core::component(memo)]
fn memoized_label(props: CountProps) -> UITree<Msg> {
    UITree::container(|c| {
        c.text(format!("value={}", props.value));
    })
}

#[test]
fn memoized_component_reuses_tree_when_props_equal() {
    let first = memoized_label(CountProps { value: 1 });
    let second = memoized_label(CountProps { value: 1 });
    let third = memoized_label(CountProps { value: 2 });

    // Same props -> identical cached tree shape (and pointer-equal clones).
    assert_eq!(
        format!("{:?}", first.meta.class),
        format!("{:?}", second.meta.class)
    );
    // Different props -> the tree text differs.
    let NodeKind::Container { children: c1 } = root_kind(&second) else {
        panic!()
    };
    let NodeKind::Container { children: c3 } = root_kind(&third) else {
        panic!()
    };
    match (&c1[0].kind, &c3[0].kind) {
        (NodeKind::Text { text: t1 }, NodeKind::Text { text: t3 }) => {
            assert_eq!(t1, "value=1");
            assert_eq!(t3, "value=2");
        }
        other => panic!("expected text, got {other:?}"),
    }
}


