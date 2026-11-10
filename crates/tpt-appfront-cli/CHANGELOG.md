# Changelog

All notable changes to `tpt-appfront-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `init --preset login|dashboard|crud-app|saas-starter` scaffolds a runnable DOM app
  from `tpt-appfront-templates` (+ `view!`) wired to `Signal`-backed state.
- `init --with login,dashboard,settings` composes an à la carte DOM app from the selected
  `tpt-appfront-templates` pieces. `--preset` and `--with` cannot be combined.
- `init --list-presets` prints both the presets and the `--with` pieces.
- `init --target server` scaffolds a smart-router server crate, and `build --target server`
  builds it in release mode.
- `dev --devtools` sets `TPT_APPFRONT_DEVTOOLS=1` on the spawned dev process so the
  scaffolded app prints its `UITree` (via `tpt_appfront_core::devtools::inspect_tree`).
- Friendlier tool-not-found errors (`missing_tool_hint`) when spawning `trunk`,
  `cargo-packager` or other external tools fail because they are not on `PATH`, and an
  explicit message when `--desktop-webview` is run without a `ui/index.html`.
- `doctor --a11y`: a deterministic, no-AST heuristic lint over the project's `src/` that
  flags `UITree` builder calls likely missing an accessible name (images with empty `alt`,
  links with empty text, buttons with an empty label). Informational only; it never fails
  the run.
- `doctor` also checks that `rustc` meets the workspace's declared `rust-version`, and that
  `wasm-bindgen-test-runner` matches the `wasm-bindgen` version in `Cargo.lock`.
- `optimize --analyze`: a heuristic scan that flags unvirtualized `List`/`DataGrid` usages
  and a release profile missing the size flags. It builds nothing and never fails.
- `generate --llm` for a live, model-backed `view!` scaffold, with `--provider`, `--model`,
  `--base-url`, `--out` and `--list-models`. It requires building the CLI with the `llm`
  feature. Providers: anthropic, openai, openrouter, grok, glm, deepseek, qwen, gemini,
  groq, mistral, together, ollama, lmstudio, azure-openai. Without `--model`, the newest
  chat model in the provider's live model list is used.
- `completions <bash|zsh|fish|powershell|elvish>` prints a shell completion script.
- `fmt` (formats the workspace and examples) and `lint` (clippy with `-D warnings`)
  subcommands.
- Canvas and TUI starters include an accessibility test.

### Changed
- `optimize --auto` builds with a size profile (opt-level `z`, LTO, one codegen unit,
  stripped) through `CARGO_PROFILE_RELEASE_*` overrides. It used to only print advice.
  `Cargo.toml` is not modified.
- `optimize --analyze` parses `Cargo.toml` as TOML and checks the values. It used to
  match substrings, so a key in a comment counted as set.
- `doctor` checks the wasm32 target from the sysroot. `rustc --print target-list` lists
  every target, installed or not, so the old check always passed.
- `doctor --fix` installs the wasm32 target and `trunk`. `doctor` also reports the webview
  runtime: GTK3 and WebKitGTK on Linux, WebView2 on Windows.
- `build` and `optimize` check their prerequisites before the long build starts: `trunk` and
  the wasm32 target for dom and webview, and `cargo-packager` for `--bundle`.
- Scaffolded projects depend on the `tpt-appfront` umbrella crate with the features they
  need, instead of on individual backend crates.
- `fmt` uses `cargo +nightly fmt`, because the repo's `rustfmt.toml` relies on unstable
  options.
- The `login` piece of `init --with` and the generated presets use the updated
  `LoginFormConfig` (`password`, `on_input`, and a plain `on_submit`), so the typed credentials
  land in the app's `Signal`s.

### Fixed
- `doctor` probes `cargo packager --version` for the cargo-packager check. Previously it only
  confirmed `cargo` was on `PATH`. A missing install is a warning, not a failure.
- `fmt --help` names `cargo +nightly fmt` to match what the command runs.

## [0.1.0]

### Added
- Initial release: `init` (canvas/dom/tui/both), `dev` (desktop/web/tui/webview,
  watch/reload loop), `build`/`optimize` (with `--bundle` via `cargo packager`),
  `benchmark`, `doctor`, `generate` (offline rule-based `view!`), `ingest` (HTML → `view!`),
  and `add component`/`add page` scaffolding. Path-dep vs published-install resolution
  for scaffolded dependencies.
