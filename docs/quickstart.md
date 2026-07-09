# Quickstart

## Install

`appfront-cli` isn't published yet, so build/install it straight from this checkout:

```sh
cargo install --path crates/appfront-cli
```

This puts an `appfront` binary on your `PATH`. It only needs to be reinstalled if the CLI itself changes — scaffolded projects don't depend on it at runtime.

You'll also want [`trunk`](https://trunkrs.dev) for the browser (DOM) target, and the `wasm32-unknown-unknown` toolchain:

```sh
cargo install trunk
rustup target add wasm32-unknown-unknown
```

## Init

```sh
appfront init myapp
```

Scaffolds `myapp/canvas` (native desktop, via `appfront-canvas`) and `myapp/dom` (browser, via `appfront-dom`) — both a working counter you can run immediately. Pass `--target canvas` or `--target dom` to scaffold just one, as a single crate at `myapp/` instead of two subdirectories.

The generated `Cargo.toml`s use `path` dependencies pointing back at this checkout's `crates/appfront-*` (they aren't on crates.io yet), so the scaffold builds with zero manual edits as long as you run `appfront init` from a machine with this repo cloned.

## Dev

```sh
appfront dev --desktop --project myapp/canvas   # native window, `cargo run`
appfront dev --web --project myapp/dom          # browser, `trunk serve`
```

`--project` defaults to `.`, so these also work run from inside the crate directory itself with no flag.

## Build

```sh
appfront build --target canvas --project myapp/canvas   # cargo build --release
appfront build --target dom --project myapp/dom         # trunk build --release
appfront build --target all --project myapp/canvas      # both, if index.html is present
```

`--target html` and `--target ai-schema` aren't standalone build artifacts — `appfront-html` and `appfront-ai-schema` are libraries you embed in your own server binary (see `appfront-server` and `crates/appfront-server/src/router.rs` for the smart-router pattern that serves all four backends from one Axum app based on client type).
