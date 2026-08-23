#!/usr/bin/env bash

set -euo pipefail

usage() {
    echo "Usage: scripts/seed-worktree.sh <new-worktree> [canonical-worktree]" >&2
}

TARGET_ROOT="${1:-}"
CANONICAL_ROOT="${2:-}"
if [[ -z "$TARGET_ROOT" || "$#" -gt 2 ]]; then
    usage
    exit 2
fi

if [[ -z "$CANONICAL_ROOT" ]]; then
    while IFS= read -r LINE; do
        case "$LINE" in
            "worktree "*)
                CANDIDATE="${LINE#worktree }"
                ;;
            "branch refs/heads/main")
                CANONICAL_ROOT="$CANDIDATE"
                break
                ;;
        esac
    done < <(git worktree list --porcelain)
fi

if [[ -z "$CANONICAL_ROOT" ]]; then
    echo "error: could not locate the canonical main worktree" >&2
    exit 1
fi

TARGET_TS="$TARGET_ROOT/shatter-ts"
CANONICAL_TS="$CANONICAL_ROOT/shatter-ts"
TARGET_LOCK="$TARGET_TS/package-lock.json"
CANONICAL_LOCK="$CANONICAL_TS/package-lock.json"
CANONICAL_MODULES="$CANONICAL_TS/node_modules"
STAMP_FILE="$TARGET_TS/node_modules/.shatter-package-lock.sha256"

if [[ ! -f "$TARGET_LOCK" ]]; then
    echo "error: target worktree has no shatter-ts/package-lock.json: $TARGET_ROOT" >&2
    exit 1
fi

LOCK_FILE="$TARGET_TS/.seed-worktree.lock"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
    echo "error: another dependency seed is already running for $TARGET_ROOT" >&2
    exit 1
fi
trap 'rm -f "$LOCK_FILE"' EXIT

START_SECONDS="$SECONDS"
LOCK_DIGEST="$(sha256sum "$TARGET_LOCK" | cut -d' ' -f1)"
record_lock_digest() {
    rm -f "$STAMP_FILE"
    printf '%s\n' "$LOCK_DIGEST" > "$STAMP_FILE"
}

run_npm_ci() {
    local reason="$1"
    echo "$reason; running npm ci"
    (
        cd "$TARGET_TS"
        npm ci --no-audit --no-fund
    )
    record_lock_digest
    echo "Installed shatter-ts/node_modules with npm ci in $((SECONDS - START_SECONDS))s"
}

if [[ -d "$TARGET_TS/node_modules" && -f "$STAMP_FILE" ]] &&
    [[ "$(<"$STAMP_FILE")" == "$LOCK_DIGEST" ]]; then
    echo "node_modules already exists in $TARGET_ROOT; leaving it unchanged"
else
    if [[ -d "$TARGET_TS/node_modules" ]]; then
        echo "Existing node_modules does not match package-lock.json; replacing it"
        rm -rf "$TARGET_TS/node_modules"
    fi

    if [[ -d "$CANONICAL_MODULES" && -f "$CANONICAL_LOCK" ]] &&
        cmp -s "$TARGET_LOCK" "$CANONICAL_LOCK"; then
        if cp -al "$CANONICAL_MODULES" "$TARGET_TS/" 2>/dev/null; then
            record_lock_digest
            echo "Seeded shatter-ts/node_modules from $CANONICAL_ROOT with hardlinks in $((SECONDS - START_SECONDS))s"
        else
            run_npm_ci "Hardlink cloning is unavailable"
        fi
    else
        run_npm_ci "Canonical dependencies are unavailable or lockfiles differ"
    fi
fi
