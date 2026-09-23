#!/usr/bin/env bash
# Filer for audit 2026-09-22 "other first-party" issue drafts.
#
# NOT executed by the drafting agent. Review the drafts first, and re-run
# bento:issue-readiness-check with a fresh reviewer on each draft before filing
# (drafts were prechecked in local-fallback mode only).
#
# Usage:
#   ./file.sh                 # dry run: print every command, touch nothing
#   ./file.sh --apply         # actually file
#   ONLY=dotfiles ./file.sh --apply   # one section: dotfiles|shatter-agents|storystore|bugshot|other
#
# Notes:
# - Beads: bodies go through --body-file (bd create --file would split on H2).
# - Beads dependency direction: `bd dep add <blocked> --blocked-by <blocker>`.
# - dotfiles uses GitHub Issues (origin git@github.com:ketang/dotfiles.git);
#   children reference the epic with a "Part of #N" line.
# - storystore's bd is write-blocked by a pending v32->v53 schema migration.
#   That section refuses to run unless STORYSTORE_MIGRATED=1 (see draft 31).
set -euo pipefail

D="$(cd "$(dirname "$0")" && pwd)"
APPLY=0
[[ "${1:-}" == "--apply" ]] && APPLY=1
ONLY="${ONLY:-all}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

want() { [[ "$ONLY" == all || "$ONLY" == "$1" ]]; }

# Extract the issue body (everything after the <!-- BODY --> marker) to a file.
body() {
  local out="$TMP/$1.body"
  awk 'f{print} /^<!-- BODY -->$/{f=1}' "$D/$1" > "$out"
  printf '%s' "$out"
}

run() {
  if (( APPLY )); then "$@"; else printf 'DRY: %q ' "$@"; echo; fi
}

# bd_create <repo_dir> <draft> <title> <type> <prio> <labels> [parent]
# Prints the new id (or a placeholder in dry-run).
bd_create() {
  local dir="$1" draft="$2" title="$3" type="$4" prio="$5" labels="$6" parent="${7:-}"
  local bf; bf="$(body "$draft")"
  local args=(bd create --title "$title" --body-file "$bf" --type "$type" --priority "$prio" --silent)
  [[ -n "$labels" ]] && args+=(--labels "$labels")
  [[ -n "$parent" ]] && args+=(--parent "$parent")
  if (( APPLY )); then
    (cd "$dir" && "${args[@]}")
  else
    printf 'DRY (cd %s): ' "$dir" >&2; printf '%q ' "${args[@]}" >&2; echo >&2
    echo "<id:${draft%%-*}>"
  fi
}

bd_note() {  # bd_note <repo_dir> <id> <text>
  if (( APPLY )); then (cd "$1" && bd note "$2" "$3"); else echo "DRY (cd $1): bd note $2 \"$3\"" >&2; fi
}

# gh_create <draft> <title> <labels> [epic_number]
gh_create() {
  local draft="$1" title="$2" labels="$3" epic="${4:-}"
  local bf; bf="$(body "$draft")"
  [[ -n "$epic" ]] && printf '\n\nPart of #%s\n' "$epic" >> "$bf"
  local args=(gh issue create -R ketang/dotfiles --title "$title" --body-file "$bf" --label "$labels")
  if (( APPLY )); then
    "${args[@]}" | sed -E 's#.*/issues/([0-9]+).*#\1#'
  else
    printf 'DRY: ' >&2; printf '%q ' "${args[@]}" >&2; echo >&2
    echo "N${draft%%-*}"
  fi
}

# ------------------------------------------------------------------ dotfiles
if want dotfiles; then
  EPIC=$(gh_create 00-dotfiles-epic.md "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)" "documentation,enhancement")
  echo "dotfiles epic: #$EPIC"
  gh_create 01-dotfiles-guidance-loading.md "Make the global Required-Loads guidance actually load (inline core rules; absolute paths)" "bug,documentation" "$EPIC"
  gh_create 02-dotfiles-background-wait.md "Add a \"waiting for background work\" rule and a no-op-poll guard hook" "enhancement,documentation" "$EPIC"
  gh_create 03-dotfiles-bypass-framing.md "Never mark a hook or gate bypass as the recommended option in AskUserQuestion" "documentation" "$EPIC"
  gh_create 04-dotfiles-plugin-autoupdate.md "Enable plugin autoUpdate for first-party marketplaces and detect stale installed plugins" "bug" "$EPIC"
  gh_create 05-dotfiles-rtk-head-range.md "rtk still shows summarized content for head -N inside compound commands (follow-up to #11)" "bug" "$EPIC"
  gh_create 06-dotfiles-memory-lifecycle.md "Add a memory lifecycle rule: tooling bugs go to the tracker; memory is a pointer that retires when the issue closes" "documentation,enhancement" "$EPIC"
  gh_create 07-dotfiles-validator-canary.md "fail-closed guidance: validators must fail on empty extraction and carry a canary test" "documentation" "$EPIC"
  gh_create 08-dotfiles-hooks-dotfiles-env.md "Global hooks reference \$DOTFILES, which is unset in many sessions" "bug" "$EPIC"
  gh_create 09-dotfiles-falsification-probe.md "Planning guidance: an experiment or benchmark plan must start with a falsification probe" "documentation" "$EPIC"
  gh_create 10-dotfiles-blocked-escalation.md "Escalation guidance when blocked on the user or on an auto-mode classifier denial" "documentation" "$EPIC"
  gh_create 11-dotfiles-tool-precedence-harness.md "Reconcile the Read/Grep-first rule with the harness bypass-mode guidance" "documentation" "$EPIC"
fi

# ------------------------------------------------------------ shatter-agents
if want shatter-agents; then
  R=/home/ketan/project/shatter-agents
  L=audit-2026-09-22
  EPIC=$(bd_create $R 20-shatter-agents-epic.md "Epic: Audit 2026-09-22 findings (shatter-agents plugin)" epic 1 "$L")
  echo "shatter-agents epic: $EPIC"
  SA21=$(bd_create $R 21-sa-shatter-diff-nonexistent.md "shatter-diff skill documents a nonexistent shatter diff <base-ref> --staged command; its pre-commit hook blocks every commit" bug 1 "$L" "$EPIC")
  SA22=$(bd_create $R 22-sa-recipe-unimplemented.md "compose-shatter-recipe and run-shatter document recipes and a stubs: registry that neither the engine nor run_targets.py implements" bug 1 "$L" "$EPIC")
  bd_create $R 23-sa-cli-contract-test.md "Add a contract test between catalog skills and the pinned shatter CLI; add requires/status metadata" task 2 "$L" "$EPIC" >/dev/null
  bd_create $R 24-sa-delegate-to-engine.md "run-shatter and shatter-doctor should call shatter list-targets / shatter doctor instead of reimplementing discovery" task 2 "$L" "$EPIC" >/dev/null
  bd_create $R 26-sa-wire-shatter-ci.md "wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main" bug 2 "$L" "$EPIC" >/dev/null
  bd_create $R 27-sa-advise-taxonomy-payload.md "shatter-advise/shatter-gaps cite a taxonomy spec that is not shipped; stale agents-arz ID; test file in payload" bug 2 "$L" "$EPIC" >/dev/null
  bd_create $R 28-sa-claude-md-import.md "CLAUDE.md points at AGENTS.md without importing it" bug 2 "$L" "$EPIC" >/dev/null
  bd_create $R 29-sa-tracker-mirrors.md "Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b)" chore 2 "$L" "$EPIC" >/dev/null
  # Notes to append to existing issues.
  bd_note $R sa-oio "$(awk 'f{print} /^<!-- BODY -->$/{f=1}' "$D/25-sa-note-sa-oio.md")"
  bd_note $R sa-tyb "Audit 2026-09-22: closed on skill-text landing, but the documented CLI does not exist (unexpected argument --staged). Follow-up: $SA21. Engine work: shatter str-81xiw."
  bd_note $R sa-yyt "Audit 2026-09-22: closed on skill-text landing; no recipe resolver/stubs config exists in the engine or run_targets.py. Follow-up: $SA22."
fi

# ---------------------------------------------------------------- storystore
if want storystore; then
  R=/home/ketan/project/storystore
  if [[ "${STORYSTORE_MIGRATED:-0}" != 1 ]]; then
    echo "SKIP storystore: bd writes blocked by pending v32->v53 schema migration."
    echo "  Run the migration per draft 31 (needs maintainer approval), then STORYSTORE_MIGRATED=1 ONLY=storystore $0 --apply"
  else
    L=audit-2026-09-22
    EPIC=$(bd_create $R 30-storystore-epic.md "Epic: Audit 2026-09-22 findings (storystore)" epic 2 "$L")
    echo "storystore epic: $EPIC"
    bd_create $R 31-ss-migration-agents-md.md "Unblock the storystore tracker (pending v32->v53 bd schema migration) and add AGENTS.md/CLAUDE.md" chore 2 "$L" "$EPIC" >/dev/null
    SS32=$(bd_create $R 32-ss-clap-cobra-extractors.md "inventory: add Rust clap and Go cobra CLI extractors; warn when a detected language has no extractor" feature 2 "$L" "$EPIC")
    bd_create $R 33-ss-version-bump.md "Adopt automatic plugin version bumps; the installed cache is 128 commits behind" chore 2 "$L" "$EPIC" >/dev/null
    bd_note $R ss-yoa "Audit 2026-09-22: extractors are still TS/JS-only for CLI surfaces (0 cli-command surfaces found in shatter). Follow-up: $SS32."
  fi
fi

# ------------------------------------------------------------------- bugshot
if want bugshot; then
  R=/home/ketan/project/bugshot
  L=audit-2026-09-22
  EPIC=$(bd_create $R 40-bugshot-epic.md "Epic: Audit 2026-09-22 findings (bugshot)" epic 2 "$L")
  echo "bugshot epic: $EPIC"
  bd_create $R 41-bgs-dedupe-mirrors.md "Close the 55 bugshot-* mirror duplicates of bgs-* issues" chore 2 "$L" "$EPIC" >/dev/null
  BG42=$(bd_create $R 42-bgs-installed-cache-bloat.md "Investigate why the installed bugshot cache is 125 MB with node_modules despite the bgs-3cz fix" task 3 "$L" "$EPIC")
  bd_create $R 43-bgs-agents-md-structure.md "AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules, sync rules and a README" chore 3 "$L" "$EPIC" >/dev/null
  bd_note $R bgs-3cz "Audit 2026-09-22: installed cache still 125 MB (node_modules, .beads, tests); marketplace still points at repo root. Investigation: $BG42."
  # Priority alignment requested by shatter (str-qwua7.53 is P2 and blocked on bgs-3tq).
  run bash -c "cd $R && bd update bgs-3tq --priority 2"
  bd_note $R bgs-3tq "Audit 2026-09-22: raised to P2 because shatter str-qwua7.53 (P2) depends on it."
fi

# --------------------------------------------------------------------- other
# goals-10: neither shatter-effectiveness (no tracker) nor holdout (empty bd)
# is a good home, so it is filed in the shatter tracker without a parent.
if want other; then
  bd_create /home/ketan/project/shatter 50-other-effectiveness-benchmark.md \
    "Deliver a minimal effectiveness benchmark (known-answer and downstream subset); retire or fix holdout" \
    task 2 "audit-2026-09-22,effectiveness" >/dev/null
fi

(( APPLY )) || echo "Dry run only. Re-run with --apply to file."
