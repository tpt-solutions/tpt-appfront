# Contributing to TPT AppFront

Thanks for your interest. This project takes **issues only** for now: bug
reports and feature requests are welcome, but pull requests are not being
accepted, so please don't open one expecting it to be reviewed or merged.

## Reporting a bug

Open an issue with the **Bug report** template. The most useful reports include:

- the backend you were using (`tpt-appfront-dom`, `-canvas`, `-tui`, `-webview`,
  `-html`, `-ai-schema`, `-server`, `-mcp`) and the target (native, `wasm32`, browser)
- the smallest `UITree` / `view!` snippet that shows the problem
- the output of `cargo test` or the browser console, if relevant
- your Rust toolchain (`rustc --version`) and OS

## Requesting a feature

Open an issue with the **Feature request** template. Describe the use case first;
a sketch of the API you have in mind is a bonus. Check [`todo.md`](todo.md) and
[`spec.txt`](spec.txt) first, since many ideas are already planned or deferred there.

## Local checks

If you want to reproduce a failure locally, the CI checks are:

```sh
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy -p tpt-appfront-dom -p tpt-appfront-canvas --target wasm32-unknown-unknown --all-targets -- -D warnings
```

`just ci` runs the same set when [`just`](https://github.com/casey/just) is installed.

## Dependency policy

The CI `deny` job runs `cargo deny check`, which enforces `deny.toml`:

- **No Apache-2.0-only crates.** Apache-2.0 stays in the allow list, so dual-licensed crates pass. Crates whose only license is Apache-2.0 are banned by name. Before adding a dependency, check its license with `cargo info <crate>`.
- **No exceptions by default.** An exception needs a `wrappers` list naming the only crates allowed to depend on the banned crate, a comment explaining why it can't be removed, and a note in the pull request for review.
- **Platform-only dependencies stay out of other platforms.** Cargo-deny merges target-specific graphs, so a dependency gated to one platform can still count everywhere. Check the merged graph with `cargo deny check`, not only the host build.
- **Run the check before opening a pull request:** `cargo deny check`. It needs the advisory database, so it may take a minute the first time.

## Licence

Issues you file are covered by the repository's dual licence (MIT or Apache-2.0),
the same as the code.
