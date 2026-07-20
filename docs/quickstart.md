# Quickstart

## Install

`tpt-appfront-cli` isn't published yet, so build/install it straight from this checkout:

```sh
cargo install --path crates/tpt-appfront-cli
```

This puts a `tpt-appfront` binary on your `PATH`. It only needs to be reinstalled if the CLI itself changes — scaffolded projects don't depend on it at runtime.

You'll also want [`trunk`](https://trunkrs.dev) for the browser (DOM) target, and the `wasm32-unknown-unknown` toolchain:

```sh
cargo install trunk
rustup target add wasm32-unknown-unknown
```

## Init

```sh
tpt-appfront init myapp
```

Scaffolds `myapp/canvas` (native desktop, via `tpt-appfront-canvas`) and `myapp/dom` (browser, via `tpt-appfront-dom`) — both a working counter you can run immediately. Pass `--target canvas` or `--target dom` to scaffold just one, as a single crate at `myapp/` instead of two subdirectories.

The generated `Cargo.toml`s use `path` dependencies pointing back at this checkout's `crates/tpt-appfront-*` (they aren't on crates.io yet), so the scaffold builds with zero manual edits as long as you run `tpt-appfront init` from a machine with this repo cloned.

## Dev

```sh
tpt-appfront dev --desktop --project myapp/canvas   # native window, `cargo run`
tpt-appfront dev --web --project myapp/dom          # browser, `trunk serve`
```

`--project` defaults to `.`, so these also work run from inside the crate directory itself with no flag.

## Build

```sh
tpt-appfront build --target canvas --project myapp/canvas   # cargo build --release
tpt-appfront build --target dom --project myapp/dom         # trunk build --release
tpt-appfront build --target all --project myapp/canvas      # both, if index.html is present
```

`--target html` and `--target ai-schema` aren't standalone build artifacts — `tpt-appfront-html` and `tpt-appfront-ai-schema` are libraries you embed in your own server binary (see `tpt-appfront-server` and `crates/tpt-appfront-server/src/router.rs` for the smart-router pattern that serves all four backends from one Axum app based on client type).

## Starter templates & migration

`tpt-appfront-templates` ships three backend-agnostic starter UIs — `login_form`, `dashboard_shell`, and `settings_list` — as stateless `UITree` builder functions you can drop into any backend (see `examples/templates-demo` for a working DOM app that composes them). The `generate` command also emits `view!` snippets for these shapes:

```sh
tpt-appfront generate --prompt "a settings list with edit and delete"   # -> view! snippet
```

Migrating an existing static/server-rendered page is a one-liner — `ingest` turns HTML into a `view!` skeleton (structure only; inline event handlers become `todo!()` stubs rather than guessed `Msg` values):

```sh
tpt-appfront ingest input.html --out src/pages/imported.rs
```

Scaffold new reusable components or route-sized pages without leaving the CLI:

```sh
tpt-appfront add component Card
tpt-appfront add page Settings
```
