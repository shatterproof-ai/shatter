#!/usr/bin/env bash
# cleanup-merged-remote-branches.sh — Delete remote branches already merged into
# the remote's main, so `git branch -r` reflects live work instead of accumulating
# dead refs. Complements scripts/cleanup.sh (which only touches LOCAL worktrees and
# branches). See AGENTS.md "Landing the Plane" for when this runs.
#
# Usage: ./scripts/cleanup-merged-remote-branches.sh [--execute] [--remote <name>]
#                                                    [--skip-bd] [-h|--help]
#
#   (no flags)      DRY RUN — list branches that WOULD be deleted, delete nothing.
#   --execute       Actually delete the safe candidates from the remote.
#   --remote <name> Remote to prune (default: origin).
#   --skip-bd       Skip the in-progress bead cross-check (worktree check only).
#                   Rarely needed: the bead check reads the tracked
#                   .beads/issues.jsonl and does NOT require a running bd/Dolt
#                   server. Use only to bypass the check entirely; less safe.
#
# SAFETY MODEL — a merged remote branch is deleted only when ALL hold:
#   * it is fully merged into <remote>/main (git for-each-ref --merged);
#   * it is not main/master/HEAD;
#   * no active git worktree is checked out on it (matched by exact name AND by
#     leading issue id, e.g. remote `str-uk8y` is protected while a worktree holds
#     `str-uk8y-config-resolution-entrypoint`);
#   * its leading issue id is not an in-progress bead (unless --skip-bd). The
#     in-progress set is read from the tracked .beads/issues.jsonl (source of
#     truth committed to git), so it works even when the Dolt server is down.
# Deleting a merged remote ref loses no work: the commits already live in main, and
# anyone holding the branch locally keeps their copy. Unmerged branches are never
# touched. When in doubt the branch is PROTECTED, not deleted.

set -euo pipefail

REMOTE="origin"
EXECUTE=false
SKIP_BD=false

while [ $# -gt 0 ]; do
  case "$1" in
    --execute)      EXECUTE=true ;;
    --skip-bd)      SKIP_BD=true ;;
    --remote)       shift; REMOTE="${1:?--remote needs a value}" ;;
    -h|--help)      sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//; /^set -euo/d'; exit 0 ;;
    *)              echo "Unknown option: $1" >&2; exit 1 ;;
  esac
  shift
done

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

log() { echo "[remote-cleanup] $*"; }

# Extract the leading beads issue id from a branch name (str-xxxx or str-xxxx.N).
issue_id() {
  printf '%s\n' "$1" | grep -oE '^str-[a-z0-9]+(\.[0-9]+)?' || true
}

log "Fetching + pruning $REMOTE ..."
git fetch --prune "$REMOTE" >/dev/null 2>&1 || {
  log "WARNING: git fetch --prune $REMOTE failed; working from local ref cache."
}

BASE="$REMOTE/main"
if ! git rev-parse --verify --quiet "$BASE" >/dev/null; then
  echo "[remote-cleanup] ERROR: $BASE not found; cannot determine merged set." >&2
  exit 1
fi

# ── Build the protected set ──────────────────────────────────────────────────
# 1. Active worktree branches (exact names + their issue ids).
protected_names=""
protected_ids=""
add_name() { protected_names="$protected_names $1"; }
add_id()   { local i; i="$(issue_id "$1")"; [ -n "$i" ] && protected_ids="$protected_ids $i"; return 0; }

while IFS= read -r wt_branch; do
  [ -z "$wt_branch" ] && continue
  add_name "$wt_branch"
  add_id "$wt_branch"
done < <(git worktree list --porcelain | sed -n 's#^branch refs/heads/##p')

# 2. In-progress beads. The tracked .beads/issues.jsonl is the source of truth
#    bd exports to and commits to git, so the in-progress set can be read
#    WITHOUT a running Dolt server — which is frequently unavailable in this
#    repo and is precisely when merged branches pile up. A live bd query is
#    consulted only as an optional freshness supplement (short timeout).
BEADS_JSONL="$PROJECT_ROOT/.beads/issues.jsonl"
if [ "$SKIP_BD" = false ]; then
  ids=""
  if [ -f "$BEADS_JSONL" ] && command -v python3 >/dev/null 2>&1; then
    log "Reading in-progress beads from $(basename "$BEADS_JSONL") (no server needed) ..."
    ids="$(python3 - "$BEADS_JSONL" <<'PY'
import json, sys
status = {}
with open(sys.argv[1]) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except ValueError:
            continue
        if isinstance(obj, dict) and "id" in obj and "status" in obj:
            status[obj["id"]] = obj["status"]  # last write wins
for issue_id, st in status.items():
    if st == "in_progress":
        print(issue_id)
PY
)"
  fi
  # Optional supplement: a live bd query, but only if the server actually
  # answers quickly. Never block cleanup on a slow/down bd.
  if command -v bd >/dev/null 2>&1; then
    bd_out="$(timeout 20 bd list --status in_progress --json 2>/dev/null || true)"
    if [ -n "$bd_out" ]; then
      ids="$ids $(printf '%s' "$bd_out" | grep -oE '"id"[[:space:]]*:[[:space:]]*"str-[a-z0-9.]+"' | grep -oE 'str-[a-z0-9.]+')"
    fi
  fi
  if [ -n "$ids" ]; then
    for id in $ids; do protected_ids="$protected_ids $id"; done
  else
    log "WARNING: no in-progress beads resolved ($(basename "$BEADS_JSONL") missing and bd unavailable); worktree check only."
  fi
else
  log "Skipping bead cross-check (--skip-bd); worktree check only."
fi

is_protected() {
  local branch="$1" id name
  case "$branch" in main|master|HEAD) return 0 ;; esac
  for name in $protected_names; do [ "$branch" = "$name" ] && return 0; done
  id="$(issue_id "$branch")"
  if [ -n "$id" ]; then
    for pid in $protected_ids; do [ "$id" = "$pid" ] && return 0; done
  fi
  return 1
}

# ── Classify merged remote branches ──────────────────────────────────────────
candidates=()
protected_hits=()
while IFS= read -r ref; do
  branch="${ref#"$REMOTE/"}"
  case "$branch" in ""|main|HEAD) continue ;; esac
  if is_protected "$branch"; then
    protected_hits+=("$branch")
  else
    candidates+=("$branch")
  fi
done < <(git for-each-ref --merged "$BASE" --format='%(refname:short)' "refs/remotes/$REMOTE")

n_cand=${#candidates[@]}
n_prot=${#protected_hits[@]}

log "Merged remote branches: $((n_cand + n_prot)) total — $n_cand deletable, $n_prot protected."
if [ "$n_prot" -gt 0 ]; then
  log "Protected (active worktree or in-progress bead):"
  for b in "${protected_hits[@]}"; do echo "    keep  $b"; done
fi

if [ "$n_cand" -eq 0 ]; then
  log "Nothing to delete. Done."
  exit 0
fi

if [ "$EXECUTE" = false ]; then
  log "DRY RUN — would delete these $n_cand branches from $REMOTE (pass --execute):"
  for b in "${candidates[@]}"; do echo "    del   $b"; done
  log "Re-run with --execute to delete."
  exit 0
fi

log "Deleting $n_cand merged branches from $REMOTE ..."
deleted=0
for b in "${candidates[@]}"; do
  if git push "$REMOTE" --delete "$b" >/dev/null 2>&1; then
    echo "    deleted $b"
    deleted=$((deleted + 1))
  else
    echo "    FAILED  $b (skipped)"
  fi
done
log "Deleted $deleted/$n_cand merged remote branches from $REMOTE."
