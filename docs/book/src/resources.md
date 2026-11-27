# Resources and Suspense

A `Resource<T>` holds a value that loads asynchronously. Its state is `Loading`, `Ready(T)` or
`Error(String)`.

```rust,ignore
use tpt_appfront_core::{Resource, Suspense};

let user: Resource<User> = Resource::new(|| load_user_blocking());

let page = Suspense::new(move || profile_view(&user))
    .track(&user)
    .fallback(|| spinner_view());

let view = page.build(); // profile_view when ready, spinner_view otherwise
```

## Loading

- `Resource::new(loader)` runs the loader straight away and stores the result.
- `load_async(spawn, loader)` hands the future to an executor you supply. The core has no runtime.
  Each load gets a generation number, and a load that finishes after a newer one started is dropped.
- `reload()` starts again and cancels any load in flight.

## Testing loads

`tpt_appfront_test::resource::ResourceDriver` runs loads without an executor. You start a load, get a
ticket, and complete it yourself, so a stale completion can be checked:

```rust,ignore
let mut d = ResourceDriver::new(Resource::ready(old));
let first = d.start();
let second = d.start();
assert!(!d.complete(first, Ok(stale))); // superseded, so it is dropped
assert!(d.complete(second, Ok(fresh)));
```

## Suspense

`Suspense::track(&resource)` records whether the resource is `Ready`. `build()` shows the content
only when every tracked resource is ready. The flag is read when `track` is called, so build a new
boundary in each render.
