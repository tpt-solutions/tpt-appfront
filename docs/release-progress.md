# Crate release progress

Tracks where each workspace crate stands on the way to a crates.io release. Status as of 2026-11-28. The "Packages (dry run)" column comes from the 2026-11-11 dry-run record in [todo.md](../todo.md) (Phase 24c), not from a re-run; re-run it before publishing.

**Overall state:** nothing has been tagged or published yet. The release workflow is dry-run by default (`.github/workflows/release.yml`, gated by the `RELEASE_PUBLISH` repo variable). Publishing waits for an explicit go-ahead (see [todo.md](../todo.md), Phase 24c).

## Summary

| Item | Status |
| --- | --- |
| Workspace version | `0.1.1` (`[workspace.package]` in root `Cargo.toml`) |
| Git tags | None yet |
| Release workflow | Written; dry run exercised locally with `cargo package --no-verify`. Not yet run on GitHub. |
| Crates packaging in dry run | 12 of 13 (umbrella `tpt-appfront` is skipped until `tpt-appfront-test` is on crates.io) |
| Crates published to crates.io | 0 |

## Per-crate status

Legend: ✅ done, ⚠️ needs attention, ⬜ not started.

Publish order follows `release.yml`. `tpt-appfront-webview` is `publish = false` and is never published.

| # | Crate | In release.yml | README | CHANGELOG | Version heading | Packages (dry run) | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `tpt-appfront-macros` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | Dependency of core, published first |
| 2 | `tpt-appfront-core` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | Root dependency. Has `tests/ui` trybuild cases (snapshots depend on rustc wording) |
| 3 | `tpt-appfront-templates` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | |
| 4 | `tpt-appfront-test` | ✅ | ✅ | ✅ | none (Unreleased only) | ✅ | Must be on crates.io before the umbrella can package. Needs a `0.1.0` heading before publish |
| 5 | `tpt-appfront-html` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | |
| 6 | `tpt-appfront-ai-schema` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | |
| 7 | `tpt-appfront-server` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | In-memory rate limiter is per-process; documented in CHANGELOG Notes |
| 8 | `tpt-appfront-tui` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | |
| 9 | `tpt-appfront-canvas` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | Windows backend verified at runtime; X11 and web only type-checked |
| 10 | `tpt-appfront-dom` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | wasm32-only. Browser tests run in CI (`wasm-tests`) |
| 11 | `tpt-appfront-mcp` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | |
| 12 | `tpt-appfront` (umbrella) | ✅ | ✅ | ✅ | none (Unreleased only) | ⬜ skipped until `tpt-appfront-test` is published | Needs a `0.1.0` heading before publish |
| 13 | `tpt-appfront-cli` | ✅ | ✅ | ✅ | `[0.1.0]` | ✅ | Last in order; depends on the published crates |
| — | `tpt-appfront-webview` | — (`publish = false`) | ✅ | ✅ | `[0.1.0]` | n/a | Not published by design. Umbrella README notes it is excluded |
| 4a | `tpt-appfront-canvas-3d` | ✅ (added 2026-11-28, after `templates`) | ✅ | ✅ (added 2026-11-28, `[Unreleased]`) | none (Unreleased only) | ⬜ not re-run | No dependencies. Needs a `0.1.0` heading and a dry-run package check before publish |

## Open items before the first publish

1. **Dry-run `tpt-appfront-canvas-3d`.** It is now in the release order and has a changelog. Run `cargo package -p tpt-appfront-canvas-3d --no-verify` to confirm it packages.
2. **Version headings.** Most changelogs have `## [0.1.0]`, but the workspace is `0.1.1`, and `tpt-appfront-test` and the umbrella have only `## [Unreleased]`. Choose one version for the first release and make every changelog and `Cargo.toml` agree.
3. **Dependency version pins.** Internal dependencies use `version = "0.1"` next to their `path`. Confirm each one resolves to the version being published.
4. **Dry run on GitHub.** Run the `release.yml` tag or manual trigger once in dry-run mode. It has only been run locally.
5. **Go-ahead to publish.** Tag and set `RELEASE_PUBLISH=true` only after the items above are done. The publish is outward-facing and cannot be undone on crates.io.
6. **Publish order.** Keep the order in `release.yml`: `tpt-appfront-test` must exist on crates.io before the umbrella packages.

## Release checklist (per crate)

- [ ] `cargo package -p <crate> --no-verify` passes (or the umbrella's skip is still justified)
- [ ] `README.md` matches the current API
- [ ] `CHANGELOG.md` has a heading for the release version, with Added/Changed/Fixed entries
- [ ] `publish = false` is set only on crates that should never ship
- [ ] Dependency order in `release.yml` still matches the crate graph
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean; wasm crates clean on `wasm32-unknown-unknown`

## Update log

- 2026-11-28: Tracker created. Nothing published. Found `tpt-appfront-canvas-3d` missing from release order and changelogs.
- 2026-11-28: Added `tpt-appfront-canvas-3d` to the `release.yml` publish order and created its `CHANGELOG.md`. It was already a workspace member, so no `Cargo.toml` change was needed.
