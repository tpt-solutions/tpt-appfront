# tpt-appfront-cli

The `tpt-appfront` command-line interface for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

One unified entry point for scaffolding, developing, building, and shipping
AppFront apps across every backend. The CLI depends only on the crates it needs
(path deps when run from the monorepo checkout, version deps on a published
install) and shells out to `cargo`/`trunk`/`cargo-packager` for the heavy lifting.

## Commands

- **`init [name]`** — scaffold a project (`--target canvas|dom|tui|both`, or
  `--preset login|dashboard|crud-app|saas-starter`). `--list-presets` prints them.
- **`dev`** — run the dev loop (`--desktop` native, `--web` trunk serve,
  `--tui` terminal, `--desktop-webview` OS webview; `--no-reload` disables the
  watch loop; `--devtools` sets `TPT_APPFRONT_DEVTOOLS=1` so the app prints its
  `UITree`).
- **`build`** — build for a target (`--target dom|canvas|webview|all`, `--bundle`
  produces installers via `cargo packager`).
- **`optimize`** — build release artifacts and report the largest size in MiB
  (`--auto`, `--bundle`).
- **`benchmark`** — run the project's `cargo bench` suites.
- **`doctor`** — pre-flight check for `trunk`, the `wasm32-unknown-unknown` target,
  `cargo-packager`, and path-vs-published dependency mode.
- **`generate`** — offline, rule-based `view!` snippet from a prompt (keyword
  matched; not a live LLM call).
- **`ingest`** — convert existing HTML into a `view!` skeleton (structure only;
  event handlers become `todo!()` stubs).
- **`add component|page`** — scaffold a reusable `view!` component or route page
  and wire it into the module tree.

## Install

```sh
cargo install --path crates/tpt-appfront-cli
```

## Example

```sh
tpt-appfront init myapp --preset dashboard
cd myapp && trunk serve
```

See [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md)
for the full workflow and [docs/troubleshooting.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/troubleshooting.md)
for common issues.

## License

MIT OR Apache-2.0
