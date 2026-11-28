# Contributing to TPT AppFront

Thanks for your interest. This project takes **issues only**: bug reports and
feature requests are welcome, but pull requests are not being accepted, so
please don't open one expecting it to be reviewed or merged.

## Before you file

- Check [`todo.md`](todo.md) and [`spec.txt`](spec.txt). Many ideas are already
  planned, deferred, or decided against there.

## Reporting a bug

Open an issue with the **Bug report** template. The most useful reports include:

- the backend you were using (`tpt-appfront-dom`, `-canvas`, `-tui`, `-webview`,
  `-html`, `-ai-schema`, `-server`, `-mcp`) and the target (native, `wasm32`, browser)
- the smallest `UITree` / `view!` snippet that shows the problem
- the output of `cargo test` or the browser console, if relevant
- your Rust toolchain (`rustc --version`) and OS

## Requesting a feature

Open an issue with the **Feature request** template. Describe the use case first;
a sketch of the API you have in mind is a bonus.

## Licence

Issues you file are covered by the repository's dual licence (MIT or Apache-2.0),
the same as the code.
