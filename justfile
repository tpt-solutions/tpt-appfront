# tpt-appfront task runner (https://github.com/casey/just)
# Install:  cargo install just
# Run `just` with no args to list targets.
#
# Shell is pinned to bash (the project's available Unix-style shell on Windows);
# the wrapper commands are plain `cargo …` invocations that work under any shell.
# set shell := ["bash", "-cu"]

# Format the workspace and every standalone example.
# Uses nightly rustfmt — rustfmt.toml sets unstable options (group_imports /
# imports_granularity / format_code_in_doc_comments) that stable ignores.
fmt:
    cargo +nightly fmt --all
    for d in examples/*/; do [ -f "$d/Cargo.toml" ] && cargo +nightly fmt --manifest-path "$d/Cargo.toml"; done

# Fail if anything is unformatted (what CI's `fmt` job enforces).
fmt-check:
    cargo +nightly fmt --all --check
    bad=0
    for d in examples/*/; do if [ -f "$d/Cargo.toml" ]; then cargo +nightly fmt --manifest-path "$d/Cargo.toml" --check || bad=1; fi; done
    [ "$bad" -eq 0 ]

# Lint the workspace (webview excluded) and every example.
lint:
    cargo clippy --workspace --all-targets --exclude tpt-appfront-webview -- -D warnings
    for d in examples/*/; do [ -f "$d/Cargo.toml" ] && cargo clippy --manifest-path "$d/Cargo.toml" --all-targets -- -D warnings; done

# Build docs and fail on broken intra-doc links.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --exclude tpt-appfront-webview

# Build against the pinned MSRV from [workspace.package].
msrv:
    cargo +1.85 build --workspace --all-targets --exclude tpt-appfront-webview

# Native test suite (webview excluded).
test:
    cargo test --workspace --exclude tpt-appfront-webview

# Run fmt-check, lint, and test together — mirrors the CI gate locally.
ci: fmt-check lint test

# Format + build + check every standalone example.
examples:
    for d in examples/*/; do if [ -f "$d/Cargo.toml" ]; then cargo fmt --manifest-path "$d/Cargo.toml"; cargo build --manifest-path "$d/Cargo.toml"; cargo fmt --manifest-path "$d/Cargo.toml" --check; fi; done
