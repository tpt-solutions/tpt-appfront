# Troubleshooting

Common issues with TPT AppFront, mostly surfaced by `tpt-appfront doctor`. Run
`doctor` first when a build/dev error looks environment-related:

```sh
tpt-appfront doctor
```

## `trunk` not found / `trunk serve` fails

The browser (DOM) target needs [`trunk`](https://trunkrs.dev):

```sh
cargo install trunk
```

`doctor` reports `trunk` as missing if it isn't on your `PATH`.

## `wasm32-unknown-unknown` target missing

DOM and wasm-canvas builds compile to `wasm32-unknown-unknown`:

```sh
rustup target add wasm32-unknown-unknown
```

`doctor` checks this and prints a `rustup target add` hint when absent.

## `cargo-packager` not found (`--bundle`)

Installer/bundling commands (`build --bundle`, `optimize --bundle`) shell out to
[`cargo-packager`](https://github.com/crabnebula-dev/cargo-packager):

```sh
cargo install cargo-packager
```

Without it, the command fails with a clear "is `cargo-packager` installed?" hint.
The raw `build`/`optimize` (no `--bundle`) still work without it.

## `tpt-appfront-dom` symbols are "missing" in the editor

`tpt-appfront-dom` is `#![cfg(target_arch = "wasm32")]` — on a native toolchain
rust-analyzer sees the empty crate and reports `mount`/`hydrate` as missing. That's
expected; point rust-analyzer at `wasm32-unknown-unknown` (see
[editor-setup.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/editor-setup.md))
to analyze DOM/wasm-canvas code. Native `cargo build` of a DOM crate also fails
for the same reason — build DOM crates with `trunk build` or
`cargo build --target wasm32-unknown-unknown`.

## Path deps vs published-version deps

Scaffolded `Cargo.toml`s use **path** dependencies when `tpt-appfront init` runs
against this monorepo checkout (a sibling `crates/tpt-appfront-core/Cargo.toml`
exists), and **version** dependencies on a published install. `doctor` prints
which mode is active.

- **Symptom:** scaffolded project fails to build with `failed to load source for
  dependency ...` / path not found.
  **Fix:** you're in published mode but the crates aren't on crates.io yet — run
  `init` from a checkout, or set `TPT_APPFRONT_DEP_VERSION` to a real version once
  the crates are published.
- **Symptom:** you changed a crate locally but a scaffolded project uses the
  published version.
  **Fix:** re-run `tpt-appfront init` from the checkout so it emits path deps, or
  edit the `Cargo.toml` to point at your local `crates/`.

## Webview (`--desktop-webview`) build fails to find a `ui/` app

`dev --desktop-webview` / `build --target webview` require a `tpt-appfront-dom`
trunk app under `ui/` (with `ui/index.html`). The CLI bails early with a clear
message if it's absent. Scaffold one:

```sh
tpt-appfront init myui --target dom
mkdir ui && cp -r myui/* ui/
```

A real window-open/click smoke test needs a display and the WebView2 (Windows) /
WebKit (macOS/Linux) runtime, which CI sandboxes don't have — `tpt-appfront-webview`
is still verified by `cargo build` + `cargo clippy` here.

## Hydration mismatch after editing a DOM app

If the server-rendered HTML and the client WASM disagree (stale state, wrong
initial values), the client falls back to re-rendering rather than attaching
listeners to mismatched nodes. Ensure the serialized `Msg`/state the client
resumes from matches what the server rendered, and that interactive subtrees are
flagged `is_dynamic` so islands hydration attaches their listeners.

## Build is slow the first time

`tpt-appfront-canvas` pulls in `egui`/`eframe`/`taffy` and `tpt-appfront-dom`
pulls in `web-sys`; first builds compile a lot of dependencies. Subsequent
builds are incremental. Use `--no-reload` with `dev --desktop` only if the
watch/reload loop interferes with an external tool.
