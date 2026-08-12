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

## `generate --llm` (live model-backed)

The default `generate` is offline and rule-based (keyword-matched against a few
known UI patterns). For open-ended prompts, pass `--llm` to call a model
provider and produce a `view!` snippet:

```sh
export ANTHROPIC_API_KEY=sk-...
tpt-appfront generate --prompt "a kanban board with drag handles" --llm
```

Five providers ship, all behind the `llm` Cargo feature:

| `--provider` | Key env var            | Default model       |
|--------------|------------------------|---------------------|
| `anthropic`  | `ANTHROPIC_API_KEY`    | `claude-sonnet-4-5` |
| `openai`     | `OPENAI_API_KEY`       | `gpt-4o`            |
| `openrouter` | `OPENROUTER_API_KEY`   | `openai/gpt-4o`     |
| `grok`       | `XAI_API_KEY`          | `grok-3`            |
| `ollama`     | *(none — local)*       | *(requires `--model`)* |

```sh
tpt-appfront generate --prompt "a dashboard with live metrics" --llm --provider openai --model gpt-4o
tpt-appfront generate --prompt "a local RAG chat" --llm --provider ollama --model llama3.1
# self-hosted / proxy endpoints:
tpt-appfront generate --prompt "..." --llm --provider openai --base-url https://my-proxy.example/v1/chat/completions
```

- **Feature-gated**: `--llm` only works when the CLI is built with the `llm`
  Cargo feature (`cargo install tpt-appfront-cli --features llm`). Without it,
  the command fails loudly — it never silently falls back to the offline
  generator.
- **Network egress + API key**: each provider calls its own endpoint. The
  matching key env var must be set (Ollama needs none, but requires an explicit
  `--model`); a missing key (or Ollama without `--model`) is a clear error — no
  fallback to another provider or to the offline generator.
- **`--provider` / `--model` / `--base-url`**: pick the provider, model id
  (falls back to the provider default except Ollama), and optionally override the
  endpoint. Adding an OpenAI-compatible provider is a one-line `PROVIDER_CONFIGS`
  table entry in `crates/tpt-appfront-cli/src/llm/mod.rs`.
- **Never hard-errors on imperfect output**: the reply's first fenced code block
  is extracted and syntax-checked with `syn`. If it doesn't parse, the snippet is
  still printed with a `// WARNING:` banner for you to fix by hand, rather than
  the command failing. Wire the emitted `Msg` variants into your app's `Msg`
  enum.

## Init presets

Instead of the bare counter, scaffold a "real" UI shape with `--preset`. Presets
wire `tpt-appfront-templates` (or the `view!` two-way-binding path) to live
`Signal`-backed state, so the result is interactive out of the box:

```sh
tpt-appfront init myapp --preset login          # sign-in form, live Signal-bound fields
tpt-appfront init myapp --preset dashboard      # nav shell composing a CRUD list
tpt-appfront init myapp --preset crud-app       # standalone CRUD list (add/edit/delete)
tpt-appfront init myapp --preset saas-starter   # full app shell, per-route content
```

A preset scaffolds a single DOM crate (`trunk serve` to run it). List them with:

```sh
tpt-appfront init --list-presets
```

## Dev: TUI target

`--tui` runs the terminal backend (`tpt-appfront-tui`) via `cargo run`. Tab/Arrows
move focus, Enter/Space activate, Esc quits:

```sh
tpt-appfront dev --tui --project myproject
```

## Dev: Webview desktop shell

`--desktop-webview` builds the hosted `ui/` trunk app (a `tpt-appfront-dom`
project) and runs the `tpt-appfront-webview` native shell that renders it in the
OS webview. Requires a `ui/index.html` next to the host crate:

```sh
tpt-appfront dev --desktop-webview --project mywebviewapp
```

`build --target webview` does the same as a one-shot release build.

## Dev: devtools inspector

`--devtools` sets `TPT_APPFRONT_DEVTOOLS=1` on the spawned dev process. The
scaffolded entry point then prints its `UITree` structure (via
`tpt_appfront_core::devtools::inspect_tree`) on startup — a quick way to inspect
the tree an app renders without a browser:

```sh
tpt-appfront dev --desktop --devtools --project myapp/canvas
```

You can also opt in at runtime by setting the env var yourself:

```sh
TPT_APPFRONT_DEVTOOLS=1 cargo run
```

## Smart router

`tpt-appfront-server` is an Axum app that serves the *same* `UITree` to four
client kinds from one endpoint, detecting the client via `User-Agent`/query param:

- **Human browser** → the WASM/HTML shell (`tpt-appfront-dom`)
- **Crawler** → semantic HTML (`tpt-appfront-html`)
- **AI agent** → JSON-LD + custom AI Schema (`tpt-appfront-ai-schema`)
- **Social bot** → OpenGraph tags (`tpt-appfront-html`)

Build a `SmartRouter` with `tpt_appfront_server::SmartRouterBuilder` (configure
static dir, wasm path, title/description, PWA, rate limiting, and an
`allowed_actions` allowlist for `POST /command`). In production, layer a TLS
terminator / reverse proxy in front — the router itself is not a hardened edge.

## Doctor (pre-flight check)

`doctor` verifies `trunk`, the `wasm32-unknown-unknown` target, and
`cargo-packager` are available, and reports whether the CLI is running against
the monorepo checkout (path deps) or a published install (version deps):

```sh
tpt-appfront doctor
```

Run it before `init`/`dev`/`build` if a toolchain error looks environment-related.
`doctor` also reports hygiene state: whether `rustfmt`/`cargo fmt` is available,
whether the local pre-commit hook is installed, and whether the workspace is
formatted.

## Formatting & linting (hygiene)

Formatting is enforced by a root `rustfmt.toml` and checked in CI (`fmt` job).
`rustfmt.toml` enables unstable options (`group_imports` / `imports_granularity`
/ `format_code_in_doc_comments`), so formatting must run on the **nightly**
rustfmt — install it once with `rustup toolchain install nightly`. The tools
below invoke `cargo +nightly fmt` for you, so you don't run plain `cargo fmt`
(stable silently drops the import grouping and CI's `fmt` job fails the diff).

```sh
tpt-appfront fmt                 # cargo +nightly fmt across workspace + examples
tpt-appfront lint                # cargo clippy -D warnings across workspace + examples
```

A `justfile` wraps the common tasks (install `just` via `cargo install just`):

```sh
just fmt          # format workspace + examples
just fmt-check    # fail if anything is unformatted (mirrors CI)
just lint         # clippy -D warnings (webview excluded)
just doc          # cargo doc with -D warnings on broken links
just test         # native test suite (webview excluded)
just ci           # fmt-check + lint + test, like the CI gate
just examples     # format + build + check every example
```

### Local pre-commit hook

`scripts/install-hooks.sh` points git at the checked-in hook in
`scripts/git-hooks/pre-commit` (which runs `cargo fmt --check`, clippy, and the
native tests). Run it once per clone:

```sh
scripts/install-hooks.sh
```

Bypass a single commit with `git commit --no-verify`.

## Benchmark

`benchmark` is the uniform entry point for a project's own `cargo bench`
suites:

```sh
tpt-appfront benchmark --project myproject
```

## Optimize & bundling

`optimize` builds release artifacts and reports the largest one's size in MiB.
The DOM/wasm template is already size-optimized (`opt-level = "z"`, `lto`,
`strip`); native (canvas/webview) builds use the crate's own
`[profile.release]`.

```sh
tpt-appfront optimize --target canvas
tpt-appfront optimize --target all
```

`--bundle` shells out to [`cargo-packager`](https://github.com/crabnebula-dev/cargo-packager)
to produce per-OS installers (.msi/.dmg/.appimage/.deb) plus delta auto-update
artifacts. It writes a `packager.toml` if one isn't present:

```sh
tpt-appfront build --target webview --bundle
tpt-appfront optimize --target canvas --bundle
```

## New `view!` tags

`view!` now covers every `NodeKind`: `Container`, `Heading`, `Text`, `Button`,
`Input`, `Textarea`, `Checkbox`, `Select`, `Radio`, `List`, and `DataGrid`.
`Textarea`/`Checkbox`/`Select`/`Radio` are self-closing and support two-way
binding via `on_input` (text/select/radio) and `on_toggle` (checkbox):

```rust
use tpt_appfront_core::{UITree, view};

let ui: UITree<Msg> = view! {
    <Container>
        <Textarea value={draft.get()} on_input={Msg::SetDraft} />
        <Checkbox label={"Subscribe"} checked={subscribed.get()} on_toggle={Msg::SetSubscribed} />
        <Select
            options={vec![("a".into(), "Apple".into())]}
            selected={choice.get()}
            on_input={Msg::Choose}
        />
        <Radio
            name={"plan".into()}
            options={vec![("free".into(), "Free".into())]}
            selected={plan.get()}
            on_input={Msg::PickPlan}
        />
    </Container>
};
```

It also supports `{if ...}`/`{for ...}` control flow and component-tag
composition (`{ my_component(props) }`). Statically-known subtrees are hoisted
into a build-once cache automatically.

## Troubleshooting

See [troubleshooting.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/troubleshooting.md)
for fixes to common issues (missing `trunk`/wasm target, path-dep vs published
install, webview toolchain, hydration mismatches).
