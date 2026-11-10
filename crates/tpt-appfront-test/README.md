# tpt-appfront-test

A headless test harness for TPT AppFront `UITree`s. It needs no browser, window or
terminal: build a tree, find nodes the way a user would, fire clicks and input, and
assert on the messages the app emitted. It also lints a tree for accessibility and AI
affordance problems.

```rust
use tpt_appfront_test::Harness;

let mut h = Harness::new(build_counter(0));
h.click("Increment");
assert_eq!(h.drain_msgs(), vec![Msg::Increment]);
```

`Msg` must be `Clone + Debug + PartialEq`. `build_counter` stands for your app's view
function.

## Public API

Re-exported from `src/lib.rs`:

- `Harness<Msg>`
  - Construction and tree: `new(root)`, `rerender(root)`, `tree()`, `snapshot()` (the
    devtools text rendering, for snapshot-style assertions).
  - Finding nodes: `find_by_text(&str)`, `find_by_role(&str)`, `find_by_key(&str)`.
  - Interaction: `click(text)`, `type_into(key, value)`, `press_key(node_key, key)`.
    Each one queues the message that the node's handler produces. Each panics if the
    node is missing or has no matching handler.
  - Messages: `drain_msgs()` takes the queued messages, `assert_msg_emitted(&msg)`
    checks one without removing it.
  - Linting: `lint()` runs `lint_tree` on the current tree.
- `lint_tree(&UITree<Msg>) -> Vec<LintIssue>` and `assert_accessible(&UITree<Msg>)`,
  which panics with every issue it finds. `LintIssue` has `rule`, `key` and `message`.
- Helpers: `role(&node)`, `text_of(&node)`, `walk(&root)`.

### Lint rules

- `unlabeled-input`: an input, textarea, select or radio has no accessible name.
- `nameless-button`: a button has no label text and no `aria-label`.
- `heading-skip`: a heading is more than one level below the previous heading.
- `duplicate-key`: two nodes share an explicit key.
- `missing-alt`: an image has empty alt text.
- `action-without-description`: an AI action has no description.

## Feature flags

- `parity`: the `parity` module. It checks whether the HTML, TUI and AI schema
  backends show the words the tree declares. `assert_parity(&ui)` panics when HTML or
  the TUI drops required text. `parity_gaps(&ui)` reports the gaps for each backend,
  including the AI schema. The canvas is not checked, because it paints text as
  captions.

## Install

```toml
[dev-dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-test = "0.1"

# for the parity checker
# tpt-appfront-test = { version = "0.1", features = ["parity"] }
```

## Example

```rust
use tpt_appfront_test::{assert_accessible, Harness};

#[test]
fn counter_emits_increment() {
    let view = build_counter(0);
    assert_accessible(&view);

    let mut h = Harness::new(view);
    h.click("Increment");
    h.assert_msg_emitted(&Msg::Increment);
}
```

## License

MIT OR Apache-2.0
