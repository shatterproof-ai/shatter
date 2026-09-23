#!/usr/bin/env bash
# Files the bento drafts from shatter audit 2026-09-22 into the bento beads tracker.
# NOT run by the drafting agent. Review drafts and run bento:issue-readiness-check
# (fresh reviewer) on each before running this.
# Usage: bash file.sh [--dry-run]
set -euo pipefail

DRAFTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO=/home/ketan/project/bento
DRY=0
[[ "${1:-}" == "--dry-run" ]] && DRY=1

cd "$REPO"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

body() {  # body <draft-file> -> path of body-only file (text after ---BODY---)
  local out="$TMP/$(basename "$1")"
  awk 'f{print} /^---BODY---$/{f=1}' "$DRAFTS/$1" > "$out"
  [[ -s "$out" ]] || { echo "empty body for $1" >&2; exit 1; }
  echo "$out"
}

run() {
  if (( DRY )); then printf 'DRY: %q ' "$@" >&2; echo >&2; echo "DRY-ID"; else "$@"; fi
}

read -r -p "Filing into $REPO beads. Readiness-checked all drafts? [y/N] " ok
[[ "$ok" == "y" ]] || { echo "aborted"; exit 1; }

# create <file> <title> <type> <priority> <labels> [extra args...]  -> echoes new id
create() {
  local f="$1" title="$2" type="$3" pri="$4" labels="$5"; shift 5
  run bd create --silent --title "$title" --body-file "$(body "$f")" \
    --type "$type" --priority "$pri" --labels "$labels" "$@"
}

EPIC=$(create 00-epic-audit-2026-09-22.md "Epic: Audit 2026-09-22 findings (bento)" epic P1 audit)
echo "epic: $EPIC"

child() { create "$@" --parent "$EPIC"; }

I01=$(child 01-git-guard-bypass-and-false-positives.md \
  "Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text" \
  bug P1 audit,hooks,safety --deps related:bento-rdtn.15)
I02=$(child 02-land-py-deletes-verifier-log.md \
  "land.py reports verifier.log as output_path after deleting it with the preview" \
  bug P1 audit,land-work --deps related:bento-rdtn.4,related:bento-rdtn.14)
I03=$(child 03-beads-hook-latency-budget.md \
  "Budget and surface beads git-hook latency in launch-work and land-work; guard slow-hook pointer cites a reference with no hooks content" \
  bug P1 audit,land-work,launch-work,hooks)
I04=$(child 04-land-work-post-push-workflow-health.md \
  "land-work: report GitHub workflow conclusions for the landed SHA; doctor flags persistently red workflows" \
  feature P1 audit,land-work,hygiene --deps related:bento-1qry)
I05=$(child 05-check-unpushed-overcount-and-landing-blocks.md \
  "check-unpushed Stop hook: count vs all remotes, skip checkouts the session did not modify, do not block during own land.py" \
  bug P2 audit,hooks,land-work --deps related:bento-neng,related:bento-k23u)
I06=$(child 06-land-py-merge-abort-ownership.md \
  "land.py aborts or hard-resets merge state in the shared primary checkout that it did not start" \
  bug P2 audit,land-work,safety)
I07=$(child 07-land-py-invocation-progress-log.md \
  "land.py: document canonical long-running invocation; emit a progress log path and heartbeat" \
  feature P2 audit,land-work,skills)
I08=$(child 08-doctor-state-per-worktree.md \
  "agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout; linked-worktree sessions never see collapsed state" \
  bug P2 audit,hooks --deps related:bento-rdtn.2)
I09=$(child 09-stale-previews-test-leak-and-scoping.md \
  "Stale land-work previews: test suite leaks into /tmp, default_preview_dir ignores TMPDIR, doctor warning unscoped and cites missing closure mode" \
  bug P2 audit,land-work,closure,hygiene --deps related:bento-rdtn.1,related:bento-7n7,related:bento-e583)
I10=$(child 10-closure-orphan-worktree-dirs.md \
  "closure: add an apply mode for orphan worktree directories the doctor flags as safe to remove" \
  feature P2 audit,closure,hygiene --deps related:bento-rdtn.1)
I11=$(child 11-landing-deletes-remote-and-superseded-branches.md \
  "land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step" \
  feature P2 audit,land-work,cleanup --deps related:bento-gd2)
I12=$(child 12-close-reason-evidence.md \
  "Close flow: reject bare Closed reasons and verify cited SHAs are ancestors of the primary branch" \
  feature P2 audit,beads-issue-flow,land-work --deps related:bento-1qry,related:bento-m4en)
I13=$(child 13-claim-branch-reconciliation.md \
  "Reconcile claims with branches: fail verify-landing when landed issue stays in_progress; report in_progress issues with no branch" \
  feature P2 audit,closure,land-work,hygiene --deps related:bento-rdtn.8,related:bento-rdtn.9)
I14=$(child 14-verifier-contract-migration.md \
  "Verifier contract drift: warn when verifier payloads lack executed; require gate output pass-through" \
  feature P2 audit,land-work --deps related:bento-rdtn.6)
I16=$(child 16-land-work-skill-restructure.md \
  "land-work SKILL.md: restructure around land.py, move manual/batch flow to references, fix \$(...) self-contradictions" \
  task P2 audit,land-work,skills --deps related:bento-by8)
I17=$(child 17-beads-snapshot-and-remote-procedure.md \
  "beads-issue-flow: define jsonl snapshot/export and Dolt-remote procedure for bd 1.x; doctor checks stale snapshot and missing remote" \
  task P2 audit,beads-issue-flow --deps related:bento-rdtn.12)
I19=$(child 19-rebase-before-land-configurable.md \
  "land.py: make rebase-before-land (--require-up-to-date) a repo policy option" \
  feature P3 audit,land-work)
I20=$(child 20-merge-push-observability.md \
  "land.py merge_push: split into timed sub-steps and capture pre-push hook stderr" \
  feature P3 audit,land-work)
I21=$(child 21-merge-message-and-stale-branch-nudge.md \
  "land.py: enforce merge-message template; nudge for aging pushed branches; one-session-per-branch guidance" \
  feature P3 audit,land-work,hygiene --deps related:bento-rdtn.9)
I22=$(child 22-followups-as-siblings.md \
  "File review follow-ups as siblings with discovered-from, not children of the closing issue; guard closing parents with open children" \
  feature P3 audit,beads-issue-flow,land-work)
I23=$(child 23-per-subagent-scratch-dirs.md \
  "swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory" \
  task P3 audit,swarm,skills --deps related:bento-btv)

# Cross-draft edges. 'blocks' direction: bd dep add <blocked> <blocker>.
run bd dep add "$I22" "$I12"                    # close helper (12) must exist before 22's guard
run bd dep add "$I02" "$I07" --type related
run bd dep add "$I05" "$I07" --type related
run bd dep add "$I20" "$I07" --type related
run bd dep add "$I16" "$I07" --type related

# Notes appended to existing issues (no new issues).
run bd comments add bento-eth  --file "$(body 15-swarm-lead-lands-from-teammate-worktree.md)"
run bd comments add bento-dyp7 --file "$(body 18-admission-control-hooks-and-land.md)"
run bd comments add bento-e583 --file "$(body 90-note-bento-e583.md)"
run bd comments add bento-a0nz --file "$(body 91-note-bento-a0nz.md)"

echo "Filed epic $EPIC and children. Review with: bd show $EPIC"
