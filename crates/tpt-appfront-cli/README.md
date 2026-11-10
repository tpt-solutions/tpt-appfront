# tpt-appfront-cli

The `tpt-appfront` command-line interface for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

It scaffolds, runs, builds and checks AppFront projects across the backends. The CLI
shells out to `cargo`, `trunk` and `cargo packager` for the heavy lifting. Scaffolded
projects depend on the `tpt-appfront` umbrella crate by path when the CLI runs from the
monorepo checkout, and by version when it is installed from a registry.

## Install

```sh
cargo install --path crates/tpt-appfront-cli
```

To use `generate --llm`, build the CLI with the `llm` feature:

```sh
cargo install --path crates/tpt-appfront-cli --features llm
```

## Prerequisites

Run `tpt-appfront doctor` to check these.

- **Rust toolchain** (`cargo`), at or above the workspace `rust-version`.
- **`wasm32-unknown-unknown` target** for DOM/web builds: `rustup target add wasm32-unknown-unknown`.
- **`trunk`** for `dev --web`, DOM builds and `dev --desktop-webview`: `cargo install trunk`.
- **`cargo-packager`** for `--bundle`: `cargo install cargo-packager`. `doctor` probes
  `cargo packager --version` and reports a missing install as a warning. It does not fail.
- **Nightly toolchain** for `fmt` (`rustup toolchain install nightly`).
- **`wasm-bindgen-test-runner`** at the version in `Cargo.lock`, for running wasm tests.
  `doctor` reports the required `cargo install` line if it is missing.

## Quick start

```sh
# Scaffold a browser (DOM) app. Defaults to `--target both` (canvas + dom), so pick one.
tpt-appfront init myapp --target dom
cd myapp

# Run it in the browser (needs trunk).
tpt-appfront dev --web

# Build a release bundle (needs trunk; the project must have index.html).
tpt-appfront build --target dom
```

For a desktop app, use `init myapp --target canvas` and then `tpt-appfront dev --desktop`.

## Commands

Commands that take `--project` default to the current directory.

### `init <name>`

Creates a new project directory `<name>`. It fails if the directory exists.

- `--target dom|canvas|tui|both|server` (default `both`). `both` creates `canvas/` and `dom/`
  subcrates.
- `--preset login|dashboard|crud-app|saas-starter`: a DOM starter app. Overrides `--target`.
- `--with login,dashboard,settings`: a DOM app composed from the selected pieces. Cannot be
  combined with `--preset`.
- `--list-presets`: prints the presets and `--with` pieces, then exits.

### `dev`

Starts the dev loop. Pass exactly one of the mode flags.

- `--desktop`: native canvas build via `cargo run`, restarted on source changes.
- `--web`: browser build via `trunk serve`.
- `--tui`: terminal build via `cargo run`, restarted on source changes.
- `--desktop-webview`: the OS webview host, with the `ui/` trunk app rebuilt and the host
  restarted on changes.
- `--no-reload`: a single run with no watch loop. It is ignored for `--web`, which always runs `trunk serve`.
- `--devtools`: sets `TPT_APPFRONT_DEVTOOLS=1`, so the app prints its `UITree` on startup.
- `--project <dir>`

### `build`

- `--target dom|canvas|tui|webview|server|all` (default `all`). `all` builds canvas, and dom
  if `index.html` exists. Aliases: `desktop` for canvas, `terminal` for tui, `wasm` for dom.
  `html`, `ssr` and `ai-schema` print guidance, since they are libraries and have no build
  artifact of their own.
- `--bundle`: runs `cargo packager` after building, and writes a `packager.toml` if the project
  has none.
- `--project <dir>`

### `optimize`

Builds release artifacts and reports the size of the largest one.

- `--target canvas|dom|webview|all` (default `all`, which is canvas, dom and webview).
- `--auto` (on by default; there is no off switch yet): builds with a size profile through `CARGO_PROFILE_RELEASE_*` overrides. `Cargo.toml` is not changed.
- `--bundle`: runs `cargo packager` after building.
- `--analyze`: a heuristic scan, with no build. It flags `List`/`DataGrid` usages that may need
  `.virtual_scroll(...)` and a release profile missing size flags.
- `--project <dir>`

### `benchmark`

Runs `cargo bench` in the project. The project must define its own benchmarks.

- `--project <dir>`

### `doctor`

Prints one line per check and exits non-zero if a required check fails. It checks the Rust
toolchain, `trunk`, the wasm target, `cargo-packager` (as noted above), whether `rustc` meets
the declared `rust-version`, the `wasm-bindgen-test-runner` version, and whether the CLI is
running from the checkout or an installed copy.

- `--a11y`: a heuristic, non-failing lint over `src/`. It flags images with empty `alt`, links
  with empty text, and buttons with an empty label.
- `--project <dir>`

### `generate --prompt "<text>"`

Writes a `view!` snippet to stdout. By default it is offline and keyword-matched, and makes no
network calls.

- `--out <file>`: write to a file instead of stdout.
- `--llm`: use a live model provider. Requires the `llm` feature.
- `--provider <name>` (default `anthropic`). The supported names are `anthropic`, `openai`,
  `openrouter`, `grok`, `glm`, `deepseek`, `qwen`, `gemini`, `groq`, `mistral`, `together`,
  `ollama`, `lmstudio` and `azure-openai`.
- `--model <id>`: the model id, or for `azure-openai`, the deployment name. If omitted, the
  newest chat model in the provider's live list is used, and the CLI refuses to guess when no
  model is clearly the newest.
- `--base-url <url>`: overrides the provider's endpoint. Required for `azure-openai`.
- `--list-models`: prints the provider's live model list and exits. `--prompt` is not needed.

API keys come from environment variables. Set the one the provider uses:
`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `XAI_API_KEY`, `ZHIPU_API_KEY`,
`DEEPSEEK_API_KEY`, `DASHSCOPE_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY`, `MISTRAL_API_KEY`,
`TOGETHER_API_KEY` or `AZURE_OPENAI_API_KEY`. `ollama` and `lmstudio` need no key.

### `ingest <input.html>`

Converts static or server-rendered HTML into a `view!` builder skeleton. It keeps the structure
and classes. Inline event handlers become `todo!()` stubs.

- `--out <file>`: write to a file instead of stdout.

### `add component|page <Name>`

Creates `src/components/<name>.rs` or `src/pages/<name>.rs` and adds the `mod` declaration to
`src/lib.rs` or `src/main.rs`. The name must be a PascalCase identifier.

- `--project <dir>`

### `fmt`

Runs `cargo +nightly fmt --all`, then formats each standalone example in `examples/`.

### `lint`

Runs `cargo clippy --workspace --all-targets --exclude tpt-appfront-webview -- -D warnings`,
then lints each standalone example with the same warnings-as-errors setting.

### `completions <shell>`

Prints a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish` to stdout.

```sh
tpt-appfront completions bash > ~/.local/share/bash-completion/completions/tpt-appfront
```

## Documentation

See [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md)
for the full workflow and [docs/troubleshooting.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/troubleshooting.md)
for common issues.

## License

MIT OR Apache-2.0
