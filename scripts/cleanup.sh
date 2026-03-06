#!/usr/bin/env bash
# cleanup.sh — Reclaim disk space from stale worktrees, build artifacts, and temp files.
# Usage: ./scripts/cleanup.sh [--dry-run] [--all] [--worktrees] [--builds] [--tmp] [--incremental]
#
# --dry-run      Show what would be cleaned without deleting anything
# --all          Clean everything (default if no flags given)
# --worktrees    Remove stale git worktrees (fully committed, no outstanding changes)
# --builds       Remove Rust target/ dirs and Go binaries
# --tmp          Remove shatter temp files from /tmp
# --incremental  Remove only Rust incremental compilation caches (fast, preserves deps)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

DRY_RUN=false
DO_WORKTREES=false
DO_BUILDS=false
DO_TMP=false
DO_INCREMENTAL=false
ANYTHING=false

for arg in "$@"; do
  case "$arg" in
    --dry-run)     DRY_RUN=true ;;
    --worktrees)   DO_WORKTREES=true; ANYTHING=true ;;
    --builds)      DO_BUILDS=true; ANYTHING=true ;;
    --tmp)         DO_TMP=true; ANYTHING=true ;;
    --incremental) DO_INCREMENTAL=true; ANYTHING=true ;;
    --all)         DO_WORKTREES=true; DO_BUILDS=true; DO_TMP=true; DO_INCREMENTAL=true; ANYTHING=true ;;
    -h|--help)
      sed -n '2,/^$/s/^# //p' "$0"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      exit 1
      ;;
  esac
done

# Default to --all if nothing specified
if [ "$ANYTHING" = false ]; then
  DO_WORKTREES=true
  DO_BUILDS=true
  DO_TMP=true
  DO_INCREMENTAL=true
fi

TOTAL_FREED=0

log()  { echo "[cleanup] $*"; }
skip() { echo "[cleanup]   skip: $*"; }

# Track bytes freed (portable: works on Linux and macOS)
dir_size_bytes() {
  du -sb "$1" 2>/dev/null | awk '{print $1}' || echo 0
}

human_size() {
  local bytes=$1
  if [ "$bytes" -ge 1073741824 ]; then
    echo "$(awk "BEGIN{printf \"%.1f\", $bytes/1073741824}")G"
  elif [ "$bytes" -ge 1048576 ]; then
    echo "$(awk "BEGIN{printf \"%.0f\", $bytes/1048576}")M"
  elif [ "$bytes" -ge 1024 ]; then
    echo "$(awk "BEGIN{printf \"%.0f\", $bytes/1024}")K"
  else
    echo "${bytes}B"
  fi
}

remove() {
  local path="$1"
  local label="${2:-$1}"
  if [ ! -e "$path" ]; then return; fi
  local size
  size=$(dir_size_bytes "$path")
  if [ "$DRY_RUN" = true ]; then
    log "would remove $label ($(human_size "$size"))"
  else
    rm -rf "$path"
    log "removed $label ($(human_size "$size"))"
  fi
  TOTAL_FREED=$((TOTAL_FREED + size))
}

# ── Stale Worktrees ──────────────────────────────────────────────────────────

if [ "$DO_WORKTREES" = true ]; then
  log "=== Worktrees ==="

  # First, prune worktrees whose directories no longer exist
  if [ "$DRY_RUN" = true ]; then
    prunable=$(git -C "$PROJECT_ROOT" worktree list --porcelain 2>/dev/null | grep -c "^prunable" || true)
    if [ "$prunable" -gt 0 ]; then
      log "would prune $prunable dead worktree references"
    fi
  else
    git -C "$PROJECT_ROOT" worktree prune 2>/dev/null && log "pruned dead worktree references"
  fi

  # Check each non-main worktree
  while IFS= read -r wt_line; do
    wt_path=$(echo "$wt_line" | awk '{print $1}')
    wt_branch=$(echo "$wt_line" | sed 's/.*\[//' | sed 's/\]//')

    # Skip the main worktree
    if [ "$wt_path" = "$PROJECT_ROOT" ]; then continue; fi

    # Check for uncommitted changes
    if git -C "$wt_path" diff --quiet 2>/dev/null && git -C "$wt_path" diff --cached --quiet 2>/dev/null; then
      # Check for untracked files (excluding .beads and .shatter runtime dirs)
      untracked=$(git -C "$wt_path" ls-files --others --exclude-standard \
        --exclude='.beads/*' --exclude='.shatter/*' 2>/dev/null | head -1)
      if [ -z "$untracked" ]; then
        size=$(dir_size_bytes "$wt_path")
        if [ "$DRY_RUN" = true ]; then
          log "would remove worktree $wt_path [$wt_branch] ($(human_size "$size"))"
        else
          git -C "$PROJECT_ROOT" worktree remove --force "$wt_path" 2>/dev/null \
            && log "removed worktree $wt_path [$wt_branch] ($(human_size "$size"))"
        fi
        TOTAL_FREED=$((TOTAL_FREED + size))
      else
        skip "$wt_path — has untracked files"
      fi
    else
      skip "$wt_path — has uncommitted changes"
    fi
  done < <(git -C "$PROJECT_ROOT" worktree list 2>/dev/null)

  # Clean empty worktree config dir
  if [ -d "$PROJECT_ROOT/.claude/worktrees" ]; then
    if [ -z "$(ls -A "$PROJECT_ROOT/.claude/worktrees" 2>/dev/null)" ]; then
      remove "$PROJECT_ROOT/.claude/worktrees" ".claude/worktrees (empty)"
    fi
  fi

  # Clean up merged branches that were used by worktrees
  while IFS= read -r branch; do
    branch=$(echo "$branch" | xargs)  # trim whitespace
    if [ -n "$branch" ] && [ "$branch" != "main" ] && [ "$branch" != "master" ]; then
      # Only delete if fully merged into main
      if [ "$DRY_RUN" = true ]; then
        log "would delete merged branch: $branch"
      else
        git -C "$PROJECT_ROOT" branch -d "$branch" 2>/dev/null \
          && log "deleted merged branch: $branch"
      fi
    fi
  done < <(git -C "$PROJECT_ROOT" branch --merged main 2>/dev/null | grep -v '^\*' | grep -v 'main')
fi

# ── Rust Build Artifacts ─────────────────────────────────────────────────────

if [ "$DO_BUILDS" = true ]; then
  log "=== Build Artifacts ==="
  remove "$PROJECT_ROOT/target" "target/ (workspace)"
  remove "$PROJECT_ROOT/shatter-rust/target" "shatter-rust/target/"

  # Go build output
  if [ -d "$PROJECT_ROOT/shatter-go/bin" ]; then
    remove "$PROJECT_ROOT/shatter-go/bin" "shatter-go/bin/"
  fi

  # Go test/build cache (project-specific only via module cache isn't safe to clear)
  # Node build output
  for dist_dir in "$PROJECT_ROOT"/shatter-ts/dist; do
    remove "$dist_dir" "shatter-ts/dist/"
  done
elif [ "$DO_INCREMENTAL" = true ]; then
  log "=== Incremental Compilation Caches ==="
  for inc_dir in \
    "$PROJECT_ROOT/target/debug/incremental" \
    "$PROJECT_ROOT/target/release/incremental" \
    "$PROJECT_ROOT/shatter-rust/target/debug/incremental" \
    "$PROJECT_ROOT/shatter-rust/target/release/incremental"; do
    remove "$inc_dir" "$(echo "$inc_dir" | sed "s|$PROJECT_ROOT/||")"
  done
fi

# ── /tmp Cleanup ─────────────────────────────────────────────────────────────

if [ "$DO_TMP" = true ]; then
  log "=== Temp Files ==="

  # Shatter-generated temp dirs/files
  for f in /tmp/shatter-gen-* /tmp/shatter-demo-* /tmp/shatter-spec* \
           /tmp/shatter-go-spec* /tmp/shatter-rust-stderr*; do
    if [ -e "$f" ]; then
      size=$(dir_size_bytes "$f")
      if [ "$DRY_RUN" = true ]; then
        log "would remove $f ($(human_size "$size"))"
      else
        rm -rf "$f"
        log "removed $f ($(human_size "$size"))"
      fi
      TOTAL_FREED=$((TOTAL_FREED + size))
    fi
  done

  # Shatter .shatter/cache dirs in examples (runtime artifacts, not seeds)
  for cache_dir in "$PROJECT_ROOT"/examples/*/.shatter/cache \
                   "$PROJECT_ROOT"/examples/*/src/.shatter/cache; do
    if [ -d "$cache_dir" ]; then
      remove "$cache_dir" "$(echo "$cache_dir" | sed "s|$PROJECT_ROOT/||")"
    fi
  done

  # Top-level .shatter/cache
  remove "$PROJECT_ROOT/.shatter/cache" ".shatter/cache"
fi

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
if [ "$DRY_RUN" = true ]; then
  log "DRY RUN — would free $(human_size $TOTAL_FREED)"
else
  log "Freed $(human_size $TOTAL_FREED)"
fi
