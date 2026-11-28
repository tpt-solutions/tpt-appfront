# Contributing to TPT AppFront

Thanks for your interest. Issues and pull requests are both welcome. For a
small fix, a pull request is the fastest route. For anything larger, open an
issue first so we can agree on the approach before you write it.

## Before you start

- Check [`todo.md`](todo.md) and [`spec.txt`](spec.txt). Many ideas are already
  planned, deferred, or decided against there.
- Work on a branch from `master`. Keep one logical change per pull request.

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

## Setting up

You need a stable Rust toolchain (the MSRV is in `Cargo.toml`) and a nightly
toolchain for formatting, because `rustfmt.toml` uses options that only nightly
supports:

```sh
rustup toolchain install nightly --component rustfmt
rustup target add wasm32-unknown-unknown
cargo install just            # optional: runs the same checks as CI
```

The `wasm32` target is needed for `tpt-appfront-dom` and the browser half of
`tpt-appfront-canvas`. The webview on Linux needs GTK 3 and WebKitGTK 4.1 at
runtime. On Debian or Ubuntu, install `libgtk-3-0 libwebkit2gtk-4.1-0`, and
`xvfb` to run its tests without a display.

## Making a change

1. Keep each crate's `CHANGELOG.md` current. Add a line under `## [Unreleased]`
   for every user-visible change, in the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
   format.
2. If your change finishes an item in `todo.md`, tick it and note what was done.
3. Write commit messages as `crate: summary`, for example
   `tui: dispatch on_input and on_toggle`. Explain the why in the body.
4. Keep backends independent. They consume `tpt-appfront-core`, and the core
   does not get backend-specific fields. See `CLAUDE.md` for the architecture.

## Checks

Run the checks CI runs before you open a pull request:

```sh
just ci            # fmt-check, lint and test for the workspace
just doc           # rustdoc with warnings as errors
```

Without `just`, the same checks are:

```sh
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy -p tpt-appfront-dom -p tpt-appfront-canvas --target wasm32-unknown-unknown --all-targets -- -D warnings
```

CI also runs the browser tests for `tpt-appfront-dom` (`wasm-bindgen-test`), the
`cli-llm` feature, the Linux webview job under `xvfb`, the macOS canvas build,
the licence and vulnerability scans, and the example builds. Some of these only
run on GitHub, so a green local run does not guarantee them. If a check fails
only in CI, say so in the pull request and include the job name.

## Pull request checklist

- [ ] One logical change, with a clear title and a description of the why.
- [ ] `just ci` passes, or the failing check is named and explained.
- [ ] `CHANGELOG.md` is updated for each crate you changed.
- [ ] `todo.md` is updated if the change finishes an item.
- [ ] New dependencies pass `cargo deny check` (see the policy below).
- [ ] Docs and examples that describe the changed behaviour are updated.

Reviews may ask for changes. Push new commits rather than rewriting history
while a review is open, so the discussion stays readable.

## Dependency policy

The CI `deny` job runs `cargo deny check`, which enforces `deny.toml`:

- **No Apache-2.0-only crates.** Apache-2.0 stays in the allow list, so dual-licensed crates pass. Crates whose only license is Apache-2.0 are banned by name. Before adding a dependency, check its license with `cargo info <crate>`.
- **No exceptions by default.** An exception needs a `wrappers` list naming the only crates allowed to depend on the banned crate, a comment explaining why it can't be removed, and a note in the pull request for review.
- **Platform-only dependencies stay out of other platforms.** Cargo-deny merges target-specific graphs, so a dependency gated to one platform can still count everywhere. Check the merged graph with `cargo deny check`, not only the host build.
- **Run the check before opening a pull request:** `cargo deny check`. It needs the advisory database, so it may take a minute the first time.

## Licence

Issues you file are covered by the repository's dual licence (MIT or Apache-2.0),
the same as the code.
