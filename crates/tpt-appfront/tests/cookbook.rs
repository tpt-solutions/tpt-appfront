//! Cookbook recipes, as tests. `docs/cookbook.md` shows the same code with commentary. Keeping
//! the recipes here means they compile and pass on every `cargo test`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use tpt_appfront::core::{
    class, ContainerBuilder, FormState, Resource, RouteTable, Router, Signal, Store, Suspense,
    UITree,
};
use tpt_appfront::prelude::view;
use tpt_appfront_html::render_page;
use tpt_appfront_test::{lint_tree, Harness};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
    Select(u32),
    Submit,
}

// ---------------------------------------------------------------------------
// 1. A counter: state in a Signal, the view is a pure function of the state.
// ---------------------------------------------------------------------------

fn counter_view(count: i32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, format!("Count: {count}"));
        c.button("Increment").on_click(Msg::Increment);
    })
}

#[test]
fn recipe_counter_click_updates_state_and_view() {
    let count = Signal::new(0i32);
    let mut h = Harness::new(counter_view(count.get()));

    h.click("Increment");
    assert_eq!(h.drain_msgs(), vec![Msg::Increment]);

    // The app handles the message: update state, then re-render.
    count.set(count.get() + 1);
    h.rerender(counter_view(count.get()));
    assert!(h.find_by_text("Count: 1").is_some());
}

// ---------------------------------------------------------------------------
// 2. A view with the `view!` macro and utility classes.
// ---------------------------------------------------------------------------

fn card() -> UITree<Msg> {
    view! {
        <Container class={ class!("p-4", "bg-white") }>
            <Heading level={2u8}>"Welcome"</Heading>
            <Button on_click={ Msg::Submit }>"Continue"</Button>
        </Container>
    }
}

#[test]
fn recipe_view_macro_with_classes() {
    let mut h = Harness::new(card());
    assert!(h.find_by_text("Welcome").is_some());
    h.click("Continue");
    h.assert_msg_emitted(&Msg::Submit);
}

// ---------------------------------------------------------------------------
// 3. A form with validation: FormState holds the values and their errors.
// ---------------------------------------------------------------------------

#[test]
fn recipe_form_validation() {
    let form = FormState::new();
    let _email = form.field("email", "");
    form.validate("email", |v| {
        (!v.contains('@')).then(|| "Enter an email address".to_string())
    });

    form.set("email", "not-an-email");
    assert_eq!(
        form.error("email").as_deref(),
        Some("Enter an email address")
    );
    assert!(!form.is_valid());

    form.set("email", "ada@example.nz");
    assert!(form.error("email").is_none());
    assert!(form.is_valid());
    assert_eq!(form.values().get("email").map(String::as_str), Some("ada@example.nz"));
}

// ---------------------------------------------------------------------------
// 4. A keyed list: each row gets a stable key, so a test (or a backend) can target one row.
// ---------------------------------------------------------------------------

fn people(items: &[(u32, &str)]) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "People");
        c.list(|list| {
            for (id, name) in items {
                list.button(*name).key(id.to_string()).on_click(Msg::Select(*id));
            }
        });
    })
}

#[test]
fn recipe_keyed_list_selects_by_row() {
    let mut h = Harness::new(people(&[(1, "Ada"), (2, "Grace")]));
    h.click("Grace");
    h.assert_msg_emitted(&Msg::Select(2));
    assert!(h.find_by_key("1").is_some());
}

// ---------------------------------------------------------------------------
// 5. Routing: map paths to views, with `:param` captures and a fallback.
// ---------------------------------------------------------------------------

/// A level-1 heading, as a whole view.
fn titled(text: &str) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, text.to_string());
    })
}

fn loading() -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.text("Loading…");
    })
}

fn home() -> UITree<Msg> {
    titled("Home")
}

fn user(params: &HashMap<String, String>) -> UITree<Msg> {
    let id = params.get("id").cloned().unwrap_or_default();
    titled(&format!("User {id}"))
}

fn not_found() -> UITree<Msg> {
    titled("Not found")
}

#[test]
fn recipe_routing_with_params_and_fallback() {
    let table = RouteTable::<Msg>::new()
        .route("/", |_| home())
        .expect("valid pattern")
        .route("/users/:id", user)
        .expect("valid pattern")
        .fallback(not_found);

    let router = Router::new(table, "/users/42");
    assert_eq!(router.current_path(), "/users/42");
    assert!(Harness::new(router.current_view()).find_by_text("User 42").is_some());

    router.navigate("/nowhere");
    assert!(Harness::new(router.current_view()).find_by_text("Not found").is_some());
}

// ---------------------------------------------------------------------------
// 6. Shared app state with a Store: subscribers run on every update.
// ---------------------------------------------------------------------------

#[test]
fn recipe_store_notifies_subscribers() {
    let store = Store::new(0i32);
    let last_seen = Rc::new(Cell::new(-1));
    let sink = last_seen.clone();
    let _subscription = store.subscribe(move |value| sink.set(*value));

    store.update(|v| v + 1);
    store.update(|v| v + 1);
    assert_eq!(store.get(), 2);
    assert_eq!(last_seen.get(), 2);
}

// ---------------------------------------------------------------------------
// 7. Async data with Resource and Suspense: show a fallback until the data is ready.
// ---------------------------------------------------------------------------

#[test]
fn recipe_resource_and_suspense() {
    let user = Resource::new(|| Ok::<String, String>("Ada".to_string()));
    let boundary = Suspense::new(|| {
        titled("Hello Ada")
    })
    .track(&user)
    .fallback(loading);
    assert!(boundary.is_ready());
    assert!(Harness::new(boundary.build()).find_by_text("Hello Ada").is_some());

    let failed: Resource<String> = Resource::error("network down");
    let boundary = Suspense::new(|| {
        titled("Hello")
    })
    .track(&failed)
    .fallback(loading);
    assert!(!boundary.is_ready());
    assert!(Harness::new(boundary.build()).find_by_text("Loading…").is_some());
}

// ---------------------------------------------------------------------------
// 8. Server-rendered HTML for crawlers and social previews.
// ---------------------------------------------------------------------------

#[test]
fn recipe_server_rendered_page() {
    let html = render_page(&counter_view(3), "Counter", "A counter example");
    assert!(html.contains("<h1"), "semantic heading in the output");
    assert!(html.contains("Count: 3"));
    assert!(html.contains("Counter"), "page title");
}

// ---------------------------------------------------------------------------
// 9. Checking a view for accessibility problems in your tests.
// ---------------------------------------------------------------------------

#[test]
fn recipe_lint_catches_an_unlabeled_input() {
    let form: UITree<Msg> = UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Sign up");
        c.input("");
        c.button("Submit").on_click(Msg::Submit);
    });
    let rules: Vec<&str> = lint_tree(&form).iter().map(|i| i.rule).collect();
    assert_eq!(rules, vec!["unlabeled-input"]);
}

// ---------------------------------------------------------------------------
// The README hero example, verbatim, through the umbrella's `test` re-export.
// ---------------------------------------------------------------------------

#[test]
fn readme_hero_example() {
    use tpt_appfront::prelude::*;
    use tpt_appfront::test::Harness;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Increment,
    }

    fn counter_view(count: i32) -> UITree<Msg> {
        UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, format!("Count: {count}"));
            c.button("Increment").on_click(Msg::Increment);
        })
    }

    let count = Signal::new(0i32);
    let mut h = Harness::new(counter_view(count.get()));
    h.click("Increment");
    assert_eq!(h.drain_msgs(), vec![Msg::Increment]);
}
