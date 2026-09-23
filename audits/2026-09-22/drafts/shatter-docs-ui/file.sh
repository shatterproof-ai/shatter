#!/usr/bin/env bash
echo "SUPERSEDED by ../../issues/INDEX.md — do not run file.sh here; use ../../issues/file-all.sh" >&2
exit 1
# Filer for audit 2026-09-22 shatter docs/UI drafts (levels L2, L3, L6).
# NOT executed by the drafting agent. Review INDEX.md first.
# Default is a dry run (prints the commands). Set APPLY=1 to file for real.
# Optional: BUMP_PRIORITY=1 also raises str-qwua7.39 to P1, as recommended in draft 03.
set -euo pipefail

DRAFTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO=/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22   # any shatter checkout; bd auto-discovers the shared DB
cd "$REPO"

APPLY="${APPLY:-0}"
run() { if [ "$APPLY" = 1 ]; then "$@"; else printf 'DRY:'; printf ' %q' "$@"; printf '\n'; fi; }

# Everything after the "<!-- body -->" marker is the issue body.
body() { local f="$DRAFTS/$1" out; out="$(mktemp)"; sed -n '/^<!-- body -->$/,$p' "$f" | tail -n +2 > "$out"; echo "$out"; }
title() { sed -n '1s/^# //p' "$DRAFTS/$1"; }

# Create an issue and print its id. In dry-run mode this prints a placeholder id.
mk() { # file type prio labels parent [deps]
  local f="$1" type="$2" prio="$3" labels="$4" parent="$5" deps="${6:-}" b
  b="$(body "$f")"
  local args=(bd create --silent --title "$(title "$f")" --body-file "$b" --type "$type" --priority "$prio" --labels "audit,$labels")
  [ -n "$parent" ] && args+=(--parent "$parent")
  [ -n "$deps" ] && args+=(--deps "$deps")
  if [ "$APPLY" = 1 ]; then "${args[@]}"; else run "${args[@]}" >&2; echo "DRYID-${f%%-*}"; fi
}
note() { # file target-id
  local b; b="$(body "$1")"
  run bd comments add "$2" -f "$b"
}

# 1. Find or create the shared top-level epic. Other audit batches may already have created it.
TOP_TITLE="Epic: Audit 2026-09-22 findings"
TOP="$(bd list --status open --json 2>/dev/null | python3 -c "import json,sys;d=json.load(sys.stdin);print(next((i['id'] for i in d if i['title']=='$TOP_TITLE'),''))")"
if [ -z "$TOP" ]; then
  tb="$(mktemp)"; cat > "$tb" <<'TXT'
Umbrella for issues filed from the 2026-09-22 full project audit (report and evidence on branch audit-2026-09-22 under audits/2026-09-22/; land that branch so the paths resolve on main). Child epics group findings by area. Issue bodies carry their own evidence inline.
TXT
  if [ "$APPLY" = 1 ]; then TOP="$(bd create --silent --title "$TOP_TITLE" --body-file "$tb" --type epic --priority 1 --labels audit)"; else run bd create --silent --title "$TOP_TITLE" --body-file "$tb" --type epic --priority 1 --labels audit; TOP=DRY-TOP; fi
fi

# 2. Child epic for this batch.
eb="$(mktemp)"; cat > "$eb" <<'TXT'
Audit 2026-09-22 findings at levels L2 (docs vs code), L3 (doc quality/IA) and L6 (CLI/report UX) for the shatter repo. See the children. Drafts, index and dedupe notes: audits/2026-09-22/drafts/shatter-docs-ui/INDEX.md on branch audit-2026-09-22.
TXT
if [ "$APPLY" = 1 ]; then
  EPIC="$(bd create --silent --title "Epic: Audit 2026-09-22 — docs accuracy and CLI/report UX" --body-file "$eb" --type epic --priority 2 --labels audit,docs,ux --parent "$TOP")"
else
  run bd create --silent --title "Epic: Audit 2026-09-22 — docs accuracy and CLI/report UX" --body-file "$eb" --type epic --priority 2 --labels audit,docs,ux --parent "$TOP"; EPIC=DRY-EPIC
fi

# 3. New issues (file, type, priority, labels, parent, deps).
mk 01-sandbox-backend-bypasses-write-guard.md     bug     1 sandbox,safety,docs,cli                     "$EPIC" related:str-qwua7.8
mk 02-explore-report-underreports-paths.md        bug     1 report,explore,ux,coverage                  "$EPIC" related:str-9q1z
mk 04-spec-changelog-backfill-false.md            bug     2 docs,spec                                   "$EPIC" related:str-qwua7.8
mk 05-artifact-schemas-and-spec-s5.md             task    2 docs,spec,schema,artifacts                  "$EPIC" related:str-qwua7.9
mk 06-spec-s6-scan-layout-checkpoint-split.md     bug     2 docs,spec,scan,artifacts,resume             "$EPIC" related:str-8q1b4
mk 07-help-leaks-execution-flags.md               bug     2 cli,ux,usability                            "$EPIC" related:str-qwua7.15
mk 08-tracker-ids-in-help-and-docs.md             task    3 cli,docs,ux                                 "$EPIC" related:str-qwua7.45
mk 09-scan-progress-post-hoc.md                   bug     2 cli,ux,progress,scan,run                    "$EPIC" related:str-7pkp.5
mk 10-rust-runtime-path-and-doctor.md             bug     2 rust-frontend,docs,ux,install               "$EPIC" related:str-qwua7.40
mk 11-per-language-outcome-rendering.md           bug     2 report,ux,parity,rust-frontend,go-frontend  "$EPIC"
mk 12-markdown-drops-render-plain-info.md         feature 3 report,ux,explore                           "$EPIC"
mk 13-minor-output-defects.md                     bug     3 report,ux,cli,demo                          "$EPIC"
mk 14-analyze-only-and-error-help-polish.md       bug     3 cli,ux,analyze,error-handling               "$EPIC" related:str-qwua7.12
mk 15-invariant-markdown-blank-subjects.md        bug     2 spec,report                                 "$EPIC" related:str-qwua7.61
mk 16-spec-yaml-custom-tags.md                    bug     2 spec,serialization,properties               "$EPIC"
mk 17-scan-report-headline-paths-zero-rows.md     bug     2 report,scan,ux                              "$EPIC"
mk 18-source-bucket-fixture-dir.md                bug     3 scan,report,classification                  "$EPIC" related:str-9awj
mk 19-control-bytes-in-reports.md                 bug     3 report,security,ux                          "$EPIC"
mk 20-branch-metric-counts-sites.md               bug     2 report,progress,coverage,ux                 "$EPIC"
mk 21-test-tier-docs-overstate-coverage.md        task    3 docs,quality-gates,taskfile,agents          "$EPIC" related:str-qwua7.2
mk 22-crate-claude-md-stale-facts.md              task    2 docs,agents,parity                          "$EPIC" related:str-qwua7.24,related:str-qwua7.25,related:str-qwua7.34
mk 24-protocol-schemas-reject-real-output.md      bug     2 protocol,schema,conformance,docs            "$EPIC" related:str-2fjn
mk 25-protocol-parity-md-stale.md                 bug     2 parity,protocol,docs                        "$EPIC" related:str-qwua7.24
mk 26-protocol-rs-doc-comments.md                 bug     3 protocol,docs,core                          "$EPIC"
mk 27-go-connection-failures-divergence.md        bug     2 go-frontend,parity,protocol,mocking         "$EPIC" related:str-2fjn
mk 28-protocol-test-doubles-relocate.md           chore   3 protocol,cleanup,tests                      "$EPIC"
mk 29-docs-smoke-coverage.md                      task    3 docs,smoke,quality-gates                    "$EPIC" related:str-qwua7.9

# 4. Notes appended to existing issues.
note 03-note-qwua7.39-json-stdout-init.md  str-qwua7.39
note 23-note-rf2v-ts-analyze-dataflow.md   str-rf2v
note 30-note-qwua7.23-agents-md.md         str-qwua7.23
if [ "${BUMP_PRIORITY:-0}" = 1 ]; then run bd update str-qwua7.39 --priority 1; fi

# Refresh the tracked snapshot. `bd sync` no longer exists in bd 1.1.0.
echo "Filed. Remember: bd export -o .beads/issues.jsonl && commit via the normal landing flow." >&2
