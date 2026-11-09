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

## Licence

Issues you file are covered by the repository's dual licence (MIT or Apache-2.0),
the same as the code.
