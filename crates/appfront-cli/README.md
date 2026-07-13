# appfront-cli

CLI for scaffolding, building, and running [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) projects.

A `clap`-based `appfront` binary with five subcommands:

- `appfront init` — scaffolds a canvas/dom/tui/webview project.
- `appfront dev --desktop|--web|--tui|--desktop-webview` — shells out to `cargo run`/`trunk serve` for a live dev loop.
- `appfront build --target <canvas|dom|tui|webview|html|ai-schema>` — shells out to `cargo build --release`/`trunk build --release` (or prints embedding guidance for the library-only `html`/`ai-schema` targets), with an optional `--bundle` flag that runs `cargo packager` for installers.
- `appfront generate --prompt "..."` — an offline, rule-based UI scaffolder that keyword-matches known patterns and emits a `view!` snippet (not a live LLM call).
- `appfront benchmark` / `appfront optimize` — wrap `cargo bench` / release-size reporting.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) and [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md) for a full walkthrough.
