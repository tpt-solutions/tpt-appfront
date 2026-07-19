# tpt-appfront-cli

The `tpt-appfront` CLI: `init`/`dev`/`build`/`generate`/`benchmark`/`optimize` for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) projects.

```sh
cargo install tpt-appfront-cli
tpt-appfront init my-app
cd my-app/canvas && cargo run          # or cd my-app/dom && trunk serve
```

- `init` scaffolds a canvas/dom/tui/webview project.
- `dev --desktop|--web|--tui|--desktop-webview` runs a dev loop (with hot-reload for `--desktop`).
- `build --target <canvas|dom|tui|webview|html|ai-schema>` builds a release artifact, optionally `--bundle`-ing installers via `cargo packager`.
- `generate --prompt "..."` emits an offline, keyword-matched `view!` UI scaffold (no LLM call).
- `benchmark` / `optimize` wrap `cargo bench` and size-optimized release builds.

See [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md) and the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme).

## License

MIT OR Apache-2.0
