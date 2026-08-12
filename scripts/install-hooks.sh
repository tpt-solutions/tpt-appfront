#!/usr/bin/env bash
# Installs the local git pre-commit hook for tpt-appfront.
#
# Instead of copying the hook into `.git/hooks` (which is per-clone and easy to
# lose on a fresh checkout), this points git at the checked-in `scripts/git-hooks`
# directory via `core.hooksPath`. The hook then lives in version control and is
# picked up by every clone.
#
# Equivalent to running:  git config core.hooksPath scripts/git-hooks

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [ ! -f scripts/git-hooks/pre-commit ]; then
    echo "error: scripts/git-hooks/pre-commit not found (run from the repo root)." >&2
    exit 1
fi

git config core.hooksPath scripts/git-hooks
echo "installed pre-commit hook via core.hooksPath = scripts/git-hooks"
echo "run \`git commit\` as usual; formatting/clippy/tests run automatically."
echo "to bypass once: git commit --no-verify"
