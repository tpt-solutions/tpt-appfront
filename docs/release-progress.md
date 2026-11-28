# Crate release progress

Tracks where each workspace crate stands on crates.io. Status as of 2026-11-28 (HEAD `b8b87b7`). Registry state was read from the crates.io API on 2026-11-28.

## Next release: 0.2.0

The unreleased changes include breaking API changes, so the next release is `0.2.0`. A patch release (`0.1.2`) would not be valid for these changes.

Breaking changes, from the `[Unreleased]` sections:

| Crate | Breaking change |
| --- | --- |
| `tpt-appfront-canvas` | `CanvasApp` and its `auto_optimize`, `frame_ms` and `optimizer_state` are removed. `OptimizerState` loses `texture_caching`. `TextMeasurer::with_font_data` is removed. The crate no longer uses `cosmic-text`. |
| `tpt-appfront-dom` | `mount` returns `Result<MountedRoot<Msg>, JsValue>` instead of its previous return type. |
| `tpt-appfront-server` | `serve()` returns `std::io::Result<()>`. Forwarded-IP handling changed: `Forwarded` is no longer read. `into_make_service_with_connect_info` is no longer needed. |
| `tpt-appfront-templates` | `LoginFormConfig` gains `password` and `on_input` fields. |
| `tpt-appfront-webview` | `WebviewOptions::allowed_actions` changes type. Not published, so no registry impact. |

The 0.2.0 bump touches these files:

1. Root `Cargo.toml`: `[workspace.package] version = "0.2.0"`.
2. Every internal dependency `version = "0.1"` becomes `version = "0.2"`. Check with `grep -rn 'version = "0.1"' crates/*/Cargo.toml crates/tpt-appfront/Cargo.toml`.
3. Each published crate's `CHANGELOG.md`: rename `## [Unreleased]` to `## [0.2.0]`, and add a new empty `## [Unreleased]` above it.
4. Umbrella `README.md` and per-crate READMEs: update any install line that pins `0.1`.
5. `release.yml`: the tag check compares against the workspace version, so the tag must be `v0.2.0`.

None of these are done yet. This file only records the plan.

## Per-crate release plan

Publish order follows `release.yml`. "Published" is the crates.io max version. Unreleased entries are counted from each `CHANGELOG.md`.

| # | Crate | Published | Unreleased entries | Breaking in 0.2.0 | Action |
| --- | --- | --- | --- | --- | --- |
| 1 | `tpt-appfront-macros` | 0.1.1 | 7 | no | Publish 0.2.0 |
| 2 | `tpt-appfront-core` | 0.1.1 | 14 | no | Publish 0.2.0 |
| 3 | `tpt-appfront-templates` | 0.1.1 | 3 | yes (`LoginFormConfig`) | Publish 0.2.0 |
| 4 | `tpt-appfront-canvas-3d` | never | 4 | n/a (new) | First publish at 0.2.0 |
| 5 | `tpt-appfront-test` | never | 4 | n/a (new) | First publish at 0.2.0. Must be on crates.io before the umbrella packages |
| 6 | `tpt-appfront-html` | 0.1.1 | 5 | no | Publish 0.2.0 |
| 7 | `tpt-appfront-ai-schema` | 0.1.1 | 5 | no | Publish 0.2.0 |
| 8 | `tpt-appfront-server` | 0.1.1 | 18 | yes (`serve()`, forwarded-IP) | Publish 0.2.0. In-memory rate limiter is per-process; documented in CHANGELOG Notes |
| 9 | `tpt-appfront-tui` | 0.1.1 | 12 | no | Publish 0.2.0 |
| 10 | `tpt-appfront-canvas` | 0.1.1 | 24 | yes (`CanvasApp` removed) | Publish 0.2.0. Windows backend verified at runtime; X11 and web only type-checked |
| 11 | `tpt-appfront-dom` | 0.1.1 | 6 | yes (`mount` return type) | Publish 0.2.0. wasm32-only; browser tests run in CI (`wasm-tests`) |
| 12 | `tpt-appfront-mcp` | 0.1.1 | 2 | no | Publish 0.2.0 |
| 13 | `tpt-appfront` (umbrella) | never | 5 | n/a (new) | First publish at 0.2.0, last. Dry-run packaging is skipped until `tpt-appfront-test` is on crates.io |
| 14 | `tpt-appfront-cli` | 0.1.1 | 30 | no | Publish 0.2.0 |
| — | `tpt-appfront-webview` | not published (`publish = false`) | 21 | yes (`allowed_actions`) | None, by design |

Every changelog needs a `[0.2.0]` heading. `canvas-3d`, `test` and the umbrella currently have only `[Unreleased]`.

## Open items

1. **Make the version bump** (steps 1–5 above). Not yet done.
2. **Changelog headings.** Rename `[Unreleased]` to `[0.2.0]` in each publishable crate. Add a `[0.1.1]` heading for the release that shipped without one, if its contents can be recovered from git history. The 0.1.1 publish has no changelog section.
3. **Dry run every publishable crate.** Run `cargo package -p <crate> --no-verify --allow-dirty` for each. `canvas-3d` and `test` have never been packaged.
4. **Confirm internal pins.** Every internal requirement reads `"0.2"` and every crate it names is in the release set.
5. **Test the release set.** Run `cargo test --workspace` and the clippy command from CLAUDE.md before tagging.
6. **Dry run on GitHub.** Run the `release.yml` manual trigger once with dry run on. It has only been run locally.
7. **Go-ahead to publish.** Set `RELEASE_PUBLISH=true` and tag `v0.2.0` only after the items above. A crates.io publish cannot be undone.

## Release checklist (per crate)

- [ ] Version is `0.2.0` in the workspace, and internal requirements are `"0.2"`
- [ ] `cargo package -p <crate> --no-verify` passes
- [ ] `README.md` matches the current API
- [ ] `CHANGELOG.md` has a `[0.2.0]` heading with Added/Changed/Removed/Fixed entries, and breaking changes are marked
- [ ] `publish = false` is set only on crates that should never ship
- [ ] Dependency order in `release.yml` matches the crate graph
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean; wasm crates clean on `wasm32-unknown-unknown`

## Registry state (2026-11-28)

- Published at 0.1.1 (12 crates): macros, core, templates, html, ai-schema, server, tui, canvas, dom, mcp, cli. 0.1.0 was published 2026-07-21 and 0.1.1 on 2026-07-27.
- Never published (3 crates): `tpt-appfront` (umbrella), `tpt-appfront-test`, `tpt-appfront-canvas-3d`.
- Not published by design: `tpt-appfront-webview`.
- No git tags exist.

Note: commit `bbd74bc` is titled "release: publish tpt-appfront-canvas-3d". That is a commit message only. It changed the workflow and changelog, and nothing was published.

## Update log

- 2026-11-28: Tracker created. Found `tpt-appfront-canvas-3d` missing from release order and changelogs; added it to `release.yml` and created its `CHANGELOG.md`.
- 2026-11-28: Corrected the tracker after a registry check. 12 of 14 publishable crates are at 0.1.1.
- 2026-11-28: Set the next release to `0.2.0` because the unreleased changes include breaking API changes. Recorded the breaking changes per crate. The version bump itself is not yet applied.
