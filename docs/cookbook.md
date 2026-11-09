# Cookbook

Short recipes for common tasks. Each one is a test in
[`crates/tpt-appfront/tests/cookbook.rs`](../crates/tpt-appfront/tests/cookbook.rs), so the code
here compiles and passes on every `cargo test`. If a recipe here and that file disagree, the file
is right and this page needs fixing.

Add the umbrella crate to a project and use the prelude:

```toml
[dependencies]
tpt-appfront = { version = "0.1", features = ["test"] }   # features as needed
```

```rust
use tpt_appfront::prelude::*;
```

The `view!` macro and `#[component]` also work with only the umbrella as a dependency.

## 1. A counter: state in a `Signal`, the view is a function of the state

Keep state outside the tree. Build the tree from the current state. When a message comes back,
update the state, then build the tree again.

```rust
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
```

Test it without a browser:

```rust
let count = Signal::new(0i32);
let mut h = Harness::new(counter_view(count.get()));
h.click("Increment");
assert_eq!(h.drain_msgs(), vec![Msg::Increment]);
count.set(count.get() + 1);
h.rerender(counter_view(count.get()));
```

## 2. The `view!` macro with utility classes

`view!` is an HTML-like template. `class!` checks utility names at compile time, so a typo such as
`class!("bg-blue-5000")` fails the build.

```rust
fn card() -> UITree<Msg> {
    view! {
        <Container class={ class!("p-4", "bg-white") }>
            <Heading level={2u8}>"Welcome"</Heading>
            <Button on_click={ Msg::Submit }>"Continue"</Button>
        </Container>
    }
}
```

Use `{ expr }` to place another `UITree` inside a `view!`. That is how you compose components.

## 3. A form with validation

`FormState` holds the field values and their validators. `set` runs the validators, and `error`
and `is_valid` read the results.

```rust
let form = FormState::new();
let _email = form.field("email", "");
form.validate("email", |v| {
    (!v.contains('@')).then(|| "Enter an email address".to_string())
});

form.set("email", "not-an-email");
assert_eq!(form.error("email").as_deref(), Some("Enter an email address"));
assert!(!form.is_valid());
```

## 4. A keyed list

Give each row a `.key(..)`. Keys let a backend move and update one row without rebuilding the
rest, and they let a test find a specific row.

```rust
c.list(|list| {
    for (id, name) in items {
        list.button(*name).key(id.to_string()).on_click(Msg::Select(*id));
    }
});
```

## 5. Routing with parameters and a fallback

```rust
let table = RouteTable::<Msg>::new()
    .route("/", |_| home())?
    .route("/users/:id", user)?   // `user` takes &HashMap<String, String>
    .fallback(not_found);

let router = Router::new(table, "/users/42");
let view = router.current_view();
router.navigate("/nowhere");
```

`navigate` updates the location signal, so effects that read `router.signal()` re-run.

## 6. Shared app state with a `Store`

```rust
let store = Store::new(0i32);
let _subscription = store.subscribe(|value| println!("now {value}"));
store.update(|v| v + 1);
```

Keep the `StoreSubscription` alive for as long as you want the callback. Dropping it unsubscribes.

## 7. Async data: `Resource` and `Suspense`

A `Resource` is `Loading`, `Ready` or `Error`. A `Suspense` boundary shows its fallback until every
resource it tracks is ready.

```rust
let user = Resource::new(|| Ok::<String, String>("Ada".to_string()));
let view = Suspense::new(|| titled("Hello Ada"))
    .track(&user)
    .fallback(|| loading())
    .build();
```

`Resource::new` runs its loader synchronously. For a real fetch, use `load_async` with your
executor, then call `set_result` when the data arrives.

## 8. Server-rendered HTML for crawlers and link previews

```rust
let html = tpt_appfront_html::render_page(&view, "Counter", "A counter example");
```

`render_page` emits semantic HTML, with OpenGraph tags for link previews. The smart router in
`tpt-appfront-server` uses the same renderer for crawlers, and the DOM backend for people.

## 9. Check a view for accessibility problems in tests

```rust
let issues = tpt_appfront_test::lint_tree(&form);
assert!(issues.iter().all(|i| i.rule != "unlabeled-input"));
```

The linter flags unlabeled inputs, buttons with no name, skipped heading levels, duplicate keys,
images without alt text, and AI actions with no description. Give an input an `aria-label` (or a
`title`) to name it.

## Where to go next

- [Quickstart](quickstart.md): install the CLI and scaffold a project.
- [Migration guides](migration/): coming from React, Leptos or Dioxus.
- [Benchmarks](benchmarks.md): what the core operations cost on a reference machine.
- [Escape hatches](escape-hatches.md): when the `UITree` model does not fit.
