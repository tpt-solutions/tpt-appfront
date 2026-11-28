# Crate release progress

Tracks where each workspace crate stands on crates.io. Status as of 2026-11-28.

## Current state: 0.2.0 released

All 14 publishable crates are on crates.io at **0.2.0**. Published by hand with `cargo publish` in dependency order on 2026-11-28, from the working tree (`--allow-dirty`). Nothing is committed, tagged, or pushed yet.

| Crate | Published |
| --- | --- |
| `tpt-appfront-macros` | 0.2.0 |
| `tpt-appfront-core` | 0.2.0 |
| `tpt-appfront-templates` | 0.2.0 |
| `tpt-appfront-canvas-3d` | 0.2.0 (first publish) |
| `tpt-appfront-html` | 0.2.0 |
| `tpt-appfront-ai-schema` | 0.2.0 |
| `tpt-appfront-tui` | 0.2.0 |
| `tpt-appfront-test` | 0.2.0 (first publish) |
| `tpt-appfront-server` | 0.2.0 |
| `tpt-appfront-canvas` | 0.2.0 |
| `tpt-appfront-dom` | 0.2.0 |
| `tpt-appfront-mcp` | 0.2.0 |
| `tpt-appfront` (umbrella) | 0.2.0 (first publish) |
| `tpt-appfront-cli` | 0.2.0 |

Not published by design: `tpt-appfront-webview` (`publish = false`).

## What changed for 0.2.0

- Workspace version `0.1.1` → `0.2.0`.
- Internal path dependencies `version = "0.1"` → `"0.2"`. README install lines updated to match.
- Every `CHANGELOG.md` has a `## [0.2.0] - 2026-11-28` heading. The unreleased entries now sit under it, and each changelog has an empty `## [Unreleased]` above.
- Breaking changes (from the changelogs):
  - `tpt-appfront-canvas`: `CanvasApp` and its optimizer fields removed; `TextMeasurer::with_font_data` removed.
  - `tpt-appfront-dom`: `mount` returns `Result<MountedRoot<Msg>, JsValue>`.
  - `tpt-appfront-server`: `serve()` returns `std::io::Result<()>`; `Forwarded` header no longer read.
  - `tpt-appfront-templates`: `LoginFormConfig` has new `password` and `on_input` fields.

## Fixed during release

- `.github/workflows/release.yml`: the publish order had `tpt-appfront-test` before `html`, `ai-schema` and `tui`, which it depends on through optional features. `cargo publish` failed on that. Order is now: macros, core, templates, canvas-3d, html, ai-schema, tui, test, server, canvas, dom, mcp, tpt-appfront, cli.

## Verification before publish

- `cargo build --workspace --all-targets`: passes.
- `cargo test --workspace`: all test binaries pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: **fails**, but not from this release:
  - Errors from a cached git checkout of `tpt-window` (`templates/*/Cargo.toml` with `{{project-name}}` names). These are outside the repo.
  - Warnings: `tpt-appfront-dom` has an unused `tpt-appfront-core` dependency on native, and `workspace.package.rust-version` is unused. Both are in code this release did not change.
- Publishing ran `cargo publish` without `--no-verify`, so each crate was verified against its registry dependencies.

## Open items

1. **Git.** Commit the release changes (workspace version, changelogs, READMEs, `release.yml` order) and tag `v0.2.0`. Nothing has been committed. Tagging is needed so that future `release.yml` runs match.
2. **Changelog `[0.1.1]` heading.** 0.1.1 was published without one. Add it if its contents can be recovered from git history.
3. **Clippy in CI.** Clippy with `-D warnings` fails on the two lint warnings above and on the template checkout errors. Fix or allow before CI runs clippy again.
4. **Webview version.** `tpt-appfront-webview` has been bumped to 0.2.0 with the workspace, but it is not published.
5. **GitHub release workflow.** Not run. Releases were published by hand, so the workflow's `RELEASE_PUBLISH` gate was not used.

## Update log

- 2026-11-28: Tracker created. Found `tpt-appfront-canvas-3d` missing from release order and changelogs; added it to `release.yml` and created its `CHANGELOG.md`.
- 2026-11-28: Corrected the tracker after a registry check. 12 of 14 publishable crates were at 0.1.1.
- 2026-11-28: Set the next release to `0.2.0` because the unreleased changes include breaking API changes.
- 2026-11-28: Bumped workspace and changelogs to 0.2.0. Build and tests passed.
- 2026-11-28: Fixed the `release.yml` publish order. Published all 14 publishable crates at 0.2.0 to crates.io.
