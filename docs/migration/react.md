# Migrating from React

## Concept map

| React | TPT AppFront |
|-------|--------------|
| Function component returning JSX | Function returning `UITree<Msg>` (`view!` or the builder) |
| `useState` | `Signal::new(v)`, read with `.get()`, write with `.set(v)` |
| `useEffect` on a value | `create_effect(move \|\| …)`; it re-runs when a signal it read changes |
| `useMemo` | `create_memo` |
| Props | Function arguments |
| `onClick={() => dispatch(...)}` | `.on_click(Msg::Variant)` on the node, plus a dispatch closure in the app |
| `key` on list items | `.key(id)` on the node |
| `useContext` / `Provider` | `provide_context` / `use_context` |
| Zustand / Redux store | `Store::new(state)` with `update` and `subscribe` |
| react-router | `RouteTable` + `Router` (`:param` captures, a fallback view) |
| react-hook-form | `FormState` (`field`, `validate`, `error`, `is_valid`) |
| `Suspense` + data fetching | `Resource` + `Suspense` (`track`, `fallback`) |
| `renderToString` (SSR) | `tpt_appfront_html::render_page` |
| Testing Library | `tpt_appfront_test::Harness` (`find_by_text`, `click`, `type_into`) |

## Side-by-side: a counter

React:

```jsx
function Counter() {
  const [count, setCount] = useState(0);
  return (
    <div>
      <h1>Count: {count}</h1>
      <button onClick={() => setCount(count + 1)}>Increment</button>
    </div>
  );
}
```

AppFront. State lives outside the view. The view is a function of the state, and a message
returns to the app to change it:

```rust
#[derive(Debug, Clone, PartialEq)]
enum Msg { Increment }

fn counter_view(count: i32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, format!("Count: {count}"));
        c.button("Increment").on_click(Msg::Increment);
    })
}

// In the app: `count` is a Signal<i32>. On Msg::Increment, call count.set(count.get() + 1),
// then rebuild the tree (or let the DOM backend's reactive_text update just the text node).
```

## Gotchas

- **No hooks.** Keep state in `Signal`s and `Store`s that you create once at startup, then pass
  them into the functions that build views. Nothing is re-created on each render, so there are
  no dependency arrays.
- **Events return messages.** A click does not run a closure inside the view. It queues a `Msg`
  that your dispatch function handles. That makes the flow easy to test: `Harness::click` then
  `drain_msgs`.
- **Re-rendering is explicit.** A view is rebuilt when you call the function again. The DOM
  backend diffs the new tree against the mounted one and keeps unchanged nodes, so rebuilding is
  cheap in practice (see [benchmarks](../benchmarks.md)). Fine-grained updates for a single
  text node are available with `reactive_text`.
- **CSS classes are typed.** `class!("p-4")` is checked at compile time. Anything that doesn't
  match the curated utility set is a build error, not a silently unstyled element. Plan a
  migration of your Tailwind classes to that set, or keep your own stylesheet and use the class
  names as plain strings.
- **Canvas and TUI are not browsers.** The same tree renders to a native window or a terminal,
  but only the DOM and HTML backends support the full node set. Check the backend matrix in the
  [README](../../README.md#backends) before you target a non-browser backend.
