# Editor Setup

Recommended VS Code extensions for working on TPT AppFront (committed at
[`.vscode/extensions.json`](https://github.com/tpt-solutions/tpt-appfront/blob/main/.vscode/extensions.json)):

- **`rust-lang.rust-analyzer`** — Rust language server: completions, go-to-def,
  inlay hints, and macro expansion.
- **`tamasfe.even-better-toml`** — TOML linting/formatting for `Cargo.toml`,
  `trunk.toml`, and `packager.toml`.
- **`serayuzgur.crates`** — inline crate documentation/version info in
  `Cargo.toml`.

A full VS Code *extension* (command palette, project scaffolding UI) is
deliberately **out of scope** for now — the `tpt-appfront` CLI already covers
those workflows from the terminal.

## Task runner (`just`)

The repo ships a [`justfile`](../justfile) (install via `cargo install just`) with
the common dev tasks — `just fmt`, `just lint`, `just test`, `just doc`, `just ci`,
and `just examples`. `just fmt` / `just fmt-check` use the **nightly** rustfmt
(because `rustfmt.toml` enables unstable import-grouping options), so install it
once with `rustup toolchain install nightly`. See
[Quickstart → Formatting & linting](../docs/quickstart.md#formatting--linting-hygiene)
for the full list.

## `view!` macro expansion

`view!` and `#[component]` are proc macros in `tpt-appfront-macros`, re-exported
through `tpt-appfront-core` (`tpt_appfront_core::view`,
`tpt_appfront_core::component`). rust-analyzer expands them on demand:

- **Expand macro recursively** (`Ctrl/Cmd+.` → *Expand macro recursively*) shows
  the generated `UITree` builder code.
- Hovering a `view!` block shows the desugared `UITree::container(|c| { ... })`
  calls — useful for confirming which node kinds/attributes a snippet lowers to.

Because the macro emits ordinary builder calls, rust-analyzer's semantic
highlighting, completions, and "go to definition" work normally inside
`{ ... }` interpolations (they're plain Rust expressions).

## `wasm32` target caveat for `tpt-appfront-dom` / `-canvas`

`tpt-appfront-dom` is conditionally compiled: it **compiles to an empty crate on
native** (`#![cfg(target_arch = "wasm32")]`) and only exposes `mount`/`hydrate`/
`reactive_text` when building for `wasm32-unknown-unknown`. The same applies to
the wasm canvas path of `tpt-appfront-canvas`.

Consequences for your editor:

- Opening `tpt-appfront-dom` **on the default (native) toolchain** makes
  rust-analyzer report `mount`, `hydrate`, etc. as *missing* — that's expected.
  rust-analyzer is analyzing the native (empty) build.
- To get editor support / type-checking for DOM and wasm-canvas code, point
  rust-analyzer at the wasm target:
  - Command Palette → **Rust Analyzer: Reveal Workspace Settings** and set
    `"rust-analyzer.cargo.target": "wasm32-unknown-unknown"`, **or**
  - Create `.vscode/settings.json`:
    ```json
    { "rust-analyzer.cargo.target": "wasm32-unknown-unknown" }
    ```
  Note this changes analysis for the *whole* workspace; switch it back to native
  when working on `tpt-appfront-core`, `-canvas` (native path), `-tui`,
  `-server`, `-cli`, or `-mcp`.

The native-only crates (`tpt-appfront-tui`, `-server`, `-cli`, `-mcp`,
`-templates`) have no such caveat and analyze normally on the default target.
