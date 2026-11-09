# tpt-appfront-test

A headless test harness for TPT AppFront `UITree`s. It needs no browser, window or
terminal: build a tree, find nodes the way a user would, fire clicks and input, and
assert on the messages the app emitted.

```rust
use tpt_appfront_test::Harness;

let mut h = Harness::new(build_counter(0));
h.click("Increment");
assert_eq!(h.drain_msgs(), vec![Msg::Increment]);
```

It also ships `lint_tree`, an accessibility and AI-affordance linter for a tree.
