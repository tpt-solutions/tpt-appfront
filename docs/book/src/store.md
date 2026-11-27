# Stores

A `Store<S>` holds shared application state. Reads and writes go through a signal, so any effect that
reads the store updates when it changes.

```rust,ignore
use tpt_appfront_core::Store;

#[derive(Clone, Default)]
struct Cart { items: Vec<String> }

let cart = Store::new(Cart::default()).with_time_travel(50);

cart.update(|c| { let mut next = c.clone(); next.items.push("Tea".into()); next });
let n = cart.get().items.len();

if cart.can_undo() { cart.undo(); }
```

- `update` takes the current state and returns the next one.
- `subscribe(callback)` runs a callback on each change. Keep the returned `StoreSubscription` alive.
- `with_time_travel(limit)` keeps up to `limit` past states for `undo` and `redo`.
- `with_persistence(backend)` saves the state through a `Persistence` backend. `WebStorage` is the
  browser implementation.
- `instrument_store` logs each change for devtools.
