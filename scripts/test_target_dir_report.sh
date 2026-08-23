#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/target-dir-report.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

WORKTREE_ONE="$SCRATCH/worktree-one"
WORKTREE_TWO="$SCRATCH/worktree-two"
mkdir -p "$WORKTREE_ONE/target" "$WORKTREE_ONE/shatter-rust/target"
mkdir -p "$WORKTREE_TWO/shatter-rust-runtime/target"
dd if=/dev/zero of="$WORKTREE_ONE/target/root.bin" bs=1024 count=4 status=none
dd if=/dev/zero of="$WORKTREE_ONE/shatter-rust/target/frontend.bin" bs=1024 count=8 status=none
dd if=/dev/zero of="$WORKTREE_TWO/shatter-rust-runtime/target/runtime.bin" bs=1024 count=16 status=none

OUTPUT="$("$SCRIPT" --worktree "$WORKTREE_ONE" --worktree "$WORKTREE_TWO")"

for EXPECTED in \
    "$WORKTREE_ONE/target" \
    "$WORKTREE_ONE/shatter-rust/target" \
    "$WORKTREE_TWO/shatter-rust-runtime/target" \
    "TOTAL"; do
    if ! printf '%s\n' "$OUTPUT" | grep -Fq "$EXPECTED"; then
        echo "[FAIL] target-dir report missing: $EXPECTED" >&2
        printf '%s\n' "$OUTPUT" >&2
        exit 1
    fi
done

echo "[ok] target-dir-report lists target directories across worktrees"
