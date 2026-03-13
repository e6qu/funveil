#!/usr/bin/env bash
# Pre-push hook: rebase current branch on origin/main and sync local main.
# Runs only for feature branches (skips if already on main).

set -euo pipefail

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"

if [ "$CURRENT_BRANCH" = "main" ]; then
    exit 0
fi

echo "Fetching origin/main..."
git fetch origin main --quiet

# Sync local main with origin/main (fast-forward only)
if git show-ref --verify --quiet refs/heads/main; then
    git update-ref refs/heads/main origin/main
fi

# Rebase current branch on top of origin/main
if ! git rebase origin/main --quiet; then
    echo "Rebase failed. Aborting rebase and push."
    git rebase --abort
    exit 1
fi

echo "Rebased $CURRENT_BRANCH on origin/main."
