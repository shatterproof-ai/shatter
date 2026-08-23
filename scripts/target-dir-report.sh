#!/usr/bin/env bash

set -euo pipefail

usage() {
    echo "Usage: scripts/target-dir-report.sh [--worktree <path>]..." >&2
}

WORKTREES=()
while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --worktree)
            if [[ "$#" -lt 2 ]]; then
                usage
                exit 2
            fi
            WORKTREES+=("$2")
            shift 2
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

if [[ "${#WORKTREES[@]}" -eq 0 ]]; then
    while IFS= read -r LINE; do
        if [[ "$LINE" == "worktree "* ]]; then
            WORKTREES+=("${LINE#worktree }")
        fi
    done < <(git worktree list --porcelain)
fi

printf 'KIB\tSIZE\tTARGET_DIR\n'
TOTAL_KIB=0
for WORKTREE in "${WORKTREES[@]}"; do
    for RELATIVE in target shatter-rust/target shatter-rust-runtime/target; do
        TARGET_DIR="$WORKTREE/$RELATIVE"
        if [[ ! -d "$TARGET_DIR" ]]; then
            continue
        fi
        if ! KIB="$(du -sk "$TARGET_DIR" | cut -f1)"; then
            echo "warning: could not measure $TARGET_DIR; skipping" >&2
            continue
        fi
        TOTAL_KIB=$((TOTAL_KIB + KIB))
        printf '%s\t%s\t%s\n' "$KIB" "$(numfmt --to=iec --suffix=B "$((KIB * 1024))")" "$TARGET_DIR"
    done
done
printf '%s\t%s\tTOTAL\n' "$TOTAL_KIB" "$(numfmt --to=iec --suffix=B "$((TOTAL_KIB * 1024))")"
