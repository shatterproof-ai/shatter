#!/usr/bin/env bash

set -euo pipefail

usage() {
    echo "Usage: scripts/target-dir-report.sh [--worktree <path>]... [--json]" >&2
}

# Kind label -> path relative to a worktree root. Order here drives the
# human-mode row order; --json mode re-sorts by kind independently.
TARGET_KINDS=(target shatter-rust/target shatter-rust-runtime/target)
TARGET_KIND_LABELS=("root target" "shatter-rust target" "runtime target")

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BEADS_JSONL="${TARGET_DIR_REPORT_BEADS_JSONL:-$REPO_ROOT/.beads/issues.jsonl}"

WORKTREES=()
JSON_MODE=false
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
        --json)
            JSON_MODE=true
            shift
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

if [[ "$JSON_MODE" == false ]]; then
    printf 'KIB\tSIZE\tTARGET_DIR\n'
    TOTAL_KIB=0
    for WORKTREE in "${WORKTREES[@]}"; do
        for RELATIVE in "${TARGET_KINDS[@]}"; do
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
    exit 0
fi

# --json mode: v1 schema
# {worktrees:[{path,branch,issue_id,issue_status,current,primary,
#              targets:[{kind,path,exists,bytes}]}], total_bytes}
#
# Bytes are logical apparent bytes: sum of st_size for regular files,
# without following symlinks, counting each directory entry once (so a
# hardlinked file contributes once per directory entry that names it, not
# deduplicated by inode). `find` without -L uses lstat for its type test,
# so -type f already excludes symlinks and never resolves them.
byte_count() {
    local dir="$1"
    find "$dir" -type f -printf '%s\n' 2>/dev/null | awk '{sum += $1} END {print sum + 0}'
}

# Resolve the real (symlink-free, absolute) path of a worktree, falling
# back to the literal argument when the directory no longer exists.
real_path() {
    local raw="$1"
    (cd "$raw" 2>/dev/null && pwd -P) || printf '%s\n' "$raw"
}

# Beads issue id embedded at the start of a branch name (str-xxxx or
# str-xxxx.N), mirroring scripts/cleanup-merged-remote-branches.sh.
issue_id_of_branch() {
    printf '%s\n' "$1" | grep -oE '^str-[a-z0-9]+(\.[0-9]+)?' || true
}

issue_status_of_id() {
    local id="$1"
    if [[ -z "$id" || ! -f "$BEADS_JSONL" ]]; then
        echo "unknown"
        return 0
    fi
    local status
    status="$(jq -r --arg id "$id" 'select(._type == "issue" and .id == $id) | .status' "$BEADS_JSONL" 2>/dev/null | head -n1)"
    if [[ -z "$status" ]]; then
        echo "unknown"
    else
        printf '%s\n' "$status"
    fi
}

# Authoritative worktree/branch/primary metadata from git itself, keyed by
# real path. The first worktree git reports is always the primary one.
declare -A PORCELAIN_BRANCH
PRIMARY_REAL_PATH=""
CUR_PATH=""
CUR_BRANCH=""
SEEN_PRIMARY=false
while IFS= read -r LINE; do
    if [[ "$LINE" == "worktree "* ]]; then
        CUR_PATH="${LINE#worktree }"
        CUR_BRANCH=""
    elif [[ "$LINE" == "branch "* ]]; then
        CUR_BRANCH="${LINE#branch refs/heads/}"
    elif [[ -z "$LINE" ]]; then
        if [[ -n "$CUR_PATH" ]]; then
            REAL="$(real_path "$CUR_PATH")"
            PORCELAIN_BRANCH["$REAL"]="$CUR_BRANCH"
            if [[ "$SEEN_PRIMARY" == false ]]; then
                PRIMARY_REAL_PATH="$REAL"
                SEEN_PRIMARY=true
            fi
        fi
        CUR_PATH=""
    fi
done < <(git worktree list --porcelain; echo)

PWD_REAL="$(pwd -P)"

WORKTREE_JSON_ITEMS=()
for WORKTREE in "${WORKTREES[@]}"; do
    REAL="$(real_path "$WORKTREE")"
    BRANCH="${PORCELAIN_BRANCH[$REAL]:-}"

    ISSUE_ID="$(issue_id_of_branch "$BRANCH")"
    if [[ -z "$ISSUE_ID" ]]; then
        ISSUE_ID_ARG=(--argjson issue_id null)
        ISSUE_STATUS="unknown"
    else
        ISSUE_ID_ARG=(--arg issue_id "$ISSUE_ID")
        ISSUE_STATUS="$(issue_status_of_id "$ISSUE_ID")"
    fi

    CURRENT=false
    [[ "$REAL" == "$PWD_REAL" ]] && CURRENT=true
    PRIMARY=false
    [[ -n "$PRIMARY_REAL_PATH" && "$REAL" == "$PRIMARY_REAL_PATH" ]] && PRIMARY=true

    TARGETS_JSON="[]"
    for I in "${!TARGET_KINDS[@]}"; do
        RELATIVE="${TARGET_KINDS[$I]}"
        KIND="${TARGET_KIND_LABELS[$I]}"
        TARGET_DIR="$WORKTREE/$RELATIVE"
        if [[ -d "$TARGET_DIR" ]]; then
            EXISTS=true
            BYTES="$(byte_count "$TARGET_DIR")"
        else
            EXISTS=false
            BYTES=0
        fi
        TARGETS_JSON="$(jq -c -n \
            --argjson prev "$TARGETS_JSON" \
            --arg kind "$KIND" \
            --arg path "$TARGET_DIR" \
            --argjson exists "$EXISTS" \
            --argjson bytes "$BYTES" \
            '$prev + [{kind: $kind, path: $path, exists: $exists, bytes: $bytes}]')"
    done
    TARGETS_JSON="$(jq -c 'sort_by(.kind)' <<<"$TARGETS_JSON")"

    WORKTREE_OBJ="$(jq -c -n \
        --arg path "$REAL" \
        --arg branch "$BRANCH" \
        "${ISSUE_ID_ARG[@]}" \
        --arg issue_status "$ISSUE_STATUS" \
        --argjson current "$CURRENT" \
        --argjson primary "$PRIMARY" \
        --argjson targets "$TARGETS_JSON" \
        '{path: $path, branch: $branch, issue_id: $issue_id, issue_status: $issue_status, current: $current, primary: $primary, targets: $targets}')"
    WORKTREE_JSON_ITEMS+=("$WORKTREE_OBJ")
done

if [[ "${#WORKTREE_JSON_ITEMS[@]}" -eq 0 ]]; then
    printf '{"worktrees":[],"total_bytes":0}\n'
    exit 0
fi

printf '%s\n' "${WORKTREE_JSON_ITEMS[@]}" | jq -s '{
    worktrees: sort_by(.path),
    total_bytes: ([.[].targets[].bytes] | add // 0)
}'
