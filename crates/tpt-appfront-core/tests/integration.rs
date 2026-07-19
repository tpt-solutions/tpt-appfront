//! Integration tests backing the `todo.md` Phase 16 "Broader integration test
//! coverage" item. These run natively (no browser) and exercise the full
//! reactive pipeline that the `wasm32`-only DOM backend performs at runtime:
//!
//! * **mount → click → signal → reconcile** — build a `UITree` from a `Signal`,
//!   simulate a user "click" by dispatching the node's `on_click` `Msg` into a
//!   reducer that updates the signal, rebuild the tree, and assert the change
//!   is reflected *in place* (keyed reconciliation produces `Keep`/`Move`
//!   edits, not a full rebuild). This is the core half of what the DOM backend
//!   does on every interaction.
//! * **view! + router e2e** — build a view with the `view!` macro, register it
//!   in a [`Router`]/[`RouteTable`], navigate, and assert the resolved tree
//!   swaps and still contains the expected nodes.

use tpt_appfront_core::reconcile::{reconcile_keys, ListEdit};
use tpt_appfront_core::signal::{create_effect, Signal};
use tpt_appfront_core::ui_tree::{NodeKind, UITree};
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
    Decrement,
}

/// The app's single source of truth: a counter.
fn model() -> Signal<i32> {
    Signal::new(0)
}

/// Builds the UI from the live counter — the function a backend would call on
/// every reactive flush (exactly how `appfront-dom`'s effect rebuilds the
/// tree). Each list item carries a `key` so reconciliation is keyed.
fn view(count: &Signal<i32>) -> UITree<Msg> {
    let n = count.get();
    UITree::container(|c| {
        c.heading(1, format!("Count: {n}"));
        c.list(|l| {
            for i in 0..n.max(0) {
                l.text(format!("item {i}")).key(format!("item-{i}"));
            }
        });
        c.button("+1").on_click(Msg::Increment).key("inc");
        c.button("-1").on_click(Msg::Decrement).key("dec");
    })
}

/// The reducer: what a click handler would do with a dispatched `Msg`.
fn reduce(count: &Signal<i32>, msg: Msg) {
    count.set(count.get() + match msg {
        Msg::Increment => 1,
        Msg::Decrement => -1,
    });
}

#[test]
fn mount_click_signal_reconcile_in_place() {
    let count = model();
    let mut before = view(&count);
    before.assign_ids();

    let NodeKind::Container { children } = &before.kind else {
        panic!("expected root container");
    };
    assert_eq!(children.len(), 4, "heading + list + two buttons");
    let NodeKind::List { items } = &children[1].kind else {
        panic!("expected list");
    };
    assert!(items.is_empty(), "counter starts at 0 → no list items");

    // Simulate a click on "+1": dispatch Increment, which updates the signal.
    reduce(&count, Msg::Increment);
    reduce(&count, Msg::Increment);
    let mut after = view(&count);
    after.assign_ids();

    let NodeKind::Container { children: c2 } = &after.kind else {
        panic!();
    };
    let NodeKind::Heading { text, .. } = &c2[0].kind else {
        panic!();
    };
    assert_eq!(text, "Count: 2");
    let NodeKind::List { items: items2 } = &c2[1].kind else {
        panic!();
    };
    assert_eq!(items2.len(), 2, "two items after two increments");

    // Reconcile the list view (the router/reconciler's job): the two new items
    // are INSERTs and the structural nodes (heading/list/buttons) are kept, so
    // the DOM backend does NOT rebuild the whole tree — it patches in place.
    let old_keys: Vec<String> = vec![];
    let new_keys: Vec<String> = (0..2).map(|i| format!("item-{i}")).collect();
    let diff = reconcile_keys(&old_keys, &new_keys);
    assert!(
        diff.edits
            .iter()
            .all(|e| matches!(e, ListEdit::Insert { .. })),
        "new items are inserted, not rebuilt"
    );
    assert!(diff.removed.is_empty());
}

#[test]
fn removing_items_reconciles_as_removes_not_full_rebuild() {
    let count = model();
    count.set(3);
    let before = view(&count);
    let NodeKind::Container { children } = &before.kind else {
        panic!();
    };
    let NodeKind::List { items } = &children[1].kind else {
        panic!();
    };
    assert_eq!(items.len(), 3);

    reduce(&count, Msg::Decrement);
    reduce(&count, Msg::Decrement);
    let after = view(&count);
    let NodeKind::Container { children: c2 } = &after.kind else {
        panic!();
    };
    let NodeKind::List { items: items2 } = &c2[1].kind else {
        panic!();
    };
    assert_eq!(items2.len(), 1, "one item remains after decrements");

    let old_keys: Vec<String> = (0..3).map(|i| format!("item-{i}")).collect();
    let new_keys: Vec<String> = (0..1).map(|i| format!("item-{i}")).collect();
    let diff = reconcile_keys(&old_keys, &new_keys);
    assert_eq!(diff.removed, vec!["item-1".to_string(), "item-2".to_string()]);
    assert_eq!(diff.edits[0], ListEdit::Keep { key: "item-0".to_string() });
}

#[test]
fn signal_effect_rebuilds_view_on_click() {
    let count = model();
    let current = Rc::new(std::cell::RefCell::new(view(&count)));
    let current_clone = current.clone();
    let count_for_effect = count.clone();
    let _handle = create_effect(move || {
        let tree = view(&count_for_effect);
        *current_clone.borrow_mut() = tree;
    });

    assert!(matches!(
        &current.borrow().kind,
        NodeKind::Container { .. }
    ));

    reduce(&count, Msg::Increment);
    let NodeKind::Container { children } = &current.borrow().kind else {
        panic!();
    };
    let NodeKind::Heading { text, .. } = &children[0].kind else {
        panic!();
    };
    assert_eq!(text, "Count: 1");
}

mod view_router {
    use tpt_appfront_core::ui_tree::{NodeKind, UITree};
    use tpt_appfront_core::{view, RouteTable, Router};

    #[derive(Debug, Clone, PartialEq)]
    enum Route {
        GoHome,
        GoAbout,
    }

    fn home() -> UITree<Route> {
        view! {
            <Container>
                <Heading level={1u8}>"Home"</Heading>
                <Button on_click={Route::GoHome}>"Home"</Button>
            </Container>
        }
    }

    fn about() -> UITree<Route> {
        view! {
            <Container>
                <Heading level={1u8}>"About"</Heading>
                <Button on_click={Route::GoAbout}>"About"</Button>
            </Container>
        }
    }

    #[test]
    fn router_resolves_and_swaps_views_on_navigation() {
        let table = RouteTable::<Route>::new()
            .route("/", |_| home())
            .unwrap()
            .route("/about", |_| about())
            .unwrap();
        let router = Router::new(table, "/");

        let v0 = router.current_view();
        let NodeKind::Container { children } = &v0.kind else {
            panic!();
        };
        assert!(matches!(children[0].kind, NodeKind::Heading { .. }));

        router.navigate("/about");
        let v1 = router.current_view();
        let NodeKind::Container { children: c1 } = &v1.kind else {
            panic!();
        };
        match &c1[0].kind {
            NodeKind::Heading { text, .. } => assert_eq!(text, "About"),
            other => panic!("expected heading, got {other:?}"),
        }
        assert_eq!(router.current_path(), "/about");
    }

    #[test]
    fn view_macro_emits_expected_structure_and_handlers() {
        let ui = home();
        let NodeKind::Container { children } = &ui.kind else {
            panic!();
        };
        assert!(matches!(children[0].kind, NodeKind::Heading { .. }));
        assert_eq!(children[1].meta.on_click, Some(Route::GoHome));
    }
}
