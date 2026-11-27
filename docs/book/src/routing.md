# Routing

A `RouteTable` maps path patterns to view functions. A `Router` holds the table and the current path.
The path is a signal, so a view that reads it updates when you navigate.

```rust,ignore
use std::collections::HashMap;
use tpt_appfront_core::{RouteTable, Router, UITree};

fn table() -> RouteTable<Msg> {
    RouteTable::new()
        .route("/", |_params: &HashMap<String, String>| home_view())
        .unwrap()
        .route("/users/:id", |params| user_view(&params["id"]))
        .unwrap()
        .fallback(not_found_view)
}

let router = Router::new(table(), "/");
router.navigate("/users/42");
let view: UITree<Msg> = router.current_view(); // user_view("42")
```

## Patterns

- Literal segments match exactly, as in `/about`.
- `:name` captures one segment into the params map. `/users/:id` matches `/users/42` with `id = "42"`.
- A trailing `*` is not supported in this version.
- `Route::parse` rejects an empty segment or a `:` with no name, and `route` returns that error.

## Per backend

- **DOM:** `mount_router` renders the current view, follows `popstate` for back and forward, and
  updates the browser URL.
- **Canvas and TUI:** both rebuild the UI from a closure on every frame. Route with
  `move || router.current_view()`, and nothing else is needed.
- **Server:** the smart router serves each request, so routing is the app's concern there.
