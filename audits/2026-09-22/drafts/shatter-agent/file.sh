#!/usr/bin/env bash
# Files the shatter AGENT-level audit drafts into the shatter bd tracker.
# DO NOT RUN until every draft passed bento:issue-readiness-check.
# Usage: bash file.sh [--dry-run]
set -euo pipefail
DRAFTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY=""; [ "${1:-}" = "--dry-run" ] && DRY="--dry-run"
cd /home/ketan/project/shatter
body() { sed -n "/^<!-- body -->$/,\$p" "$DRAFTS/$1" | tail -n +2; }
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
declare -A ID

create() {  # num file title type prio labels [parent]
  local num="$1" file="$2" title="$3" type="$4" prio="$5" labels="$6" parent="${7:-}"
  body "$file" > "$tmp/$num.md"
  local args=(create --title "$title" --type "$type" --priority "$prio" --labels "$labels" --body-file "$tmp/$num.md" --silent)
  [ -n "$parent" ] && args+=(--parent "$parent")
  if [ -n "$DRY" ]; then echo "bd ${args[*]}"; ID[$num]="DRY-$num"; return; fi
  ID[$num]="$(bd "${args[@]}")"
  echo "$num -> ${ID[$num]}"
}

note() {  # num file target
  body "$2" > "$tmp/$1.md"
  if [ -n "$DRY" ]; then echo "bd update $3 --append-notes <$2>"; return; fi
  bd update "$3" --append-notes "$(cat "$tmp/$1.md")"
  echo "$1 -> appended to $3"
}

create 01 '01-epic-audit-2026-09-22-agent.md' 'Epic: Audit 2026-09-22 findings (agent system / process)' epic P1 'audit,agents'
create 02 '02-drift-patrol-workflow-never-runs.md' 'Fix scheduled Drift Patrol workflow (setup-go points at nonexistent root go.mod; 7/7 scheduled runs red)' bug P1 'agents,ci,github-actions,drift' "${ID[01]}"
create 03 '03-workflow-health-signal.md' 'Surface persistently red GitHub workflows to agents (Build and Release 0/200, Perf CI 0/13, no issue)' task P1 'agents,ci,github-actions,drift,landing' "${ID[01]}"
create 04 '04-repair-poisoned-git-identity.md' 'Remove leaked fixture identity (Test <test@example.com>) from primary .git/config and add a repo-state check' bug P1 'agents,git,tooling,drift' "${ID[01]}"
create 05 '05-bd-sync-removed-jsonl-stale.md' 'Replace removed `bd sync` in agent docs and refresh the frozen .beads/issues.jsonl snapshot (plus a freshness check)' bug P1 'agents,beads,docs,drift' "${ID[01]}"
create 06 '06-publish-audit-reports-and-audit-skill-landing.md' 'Land the 2026-09-04 audit report on main and make /audit publish before filing' task P1 'agents,audit,skills,docs' "${ID[01]}"
create 07 '07-repair-stale-agent-memory.md' 'Rewrite stale shatter agent memories that prescribe --no-verify/hooksPath bypass and state false repo facts' task P1 'agents,memory,git-hooks' "${ID[01]}"
create 08 '08-beads-hook-timeout-decision.md' 'Decide and apply beads hook timeout policy (post-checkout ~300s per worktree/preview; str-qwua7.28 vs str-mpgg1 deadlock)' decision P1 'agents,beads,git-hooks,landing' "${ID[01]}"
create 09 '09-gauntlet-scan-checker-dead.md' 'Gauntlet scan-failure checker has matched nothing since 2026-05-13; tests pin the dead format' bug P1 'gauntlet,quality-gates,testing,agents' "${ID[01]}"
create 10 '10-tracker-reconciliation-sweep.md' 'Tracker reconciliation: close resolved/obsolete/landed issues and add a landed-not-closed patrol check' chore P2 'agents,beads,drift,governance' "${ID[01]}"
create 11 '11-triage-policy-and-audit-epic-waves.md' 'Define P1, cap open P1s, and split/wave-order the stalled str-qwua7 audit epic' task P2 'agents,governance,audit,drift' "${ID[01]}"
create 12 '12-fixture-corruption-incident-and-reverify.md' 'Record the 2026-09-07 fixture-corruption incident, review recovery branches, and re-verify str-qwua7.14 on origin/main' task P2 'agents,git,governance' "${ID[01]}"
create 13 '13-global-gitignore-hides-agent-config.md' 'Make .claude/ and .codex/ agent config trackable despite the global gitignore' task P2 'agents,git,skills' "${ID[01]}"
create 14 '14-repo-skills-rot.md' 'Repair rotted repo skills (check-go/rust/ts bare commands, superseded protocol-sync, audit skill paths/steps)' task P2 'agents,skills,docs' "${ID[01]}"
create 15 '15-parity-guidance-skill-and-template.md' 'Rewrite frontend-parity skill workflow and extend frontend-issue-template parity checklist to Go/Rust builders' task P3 'agents,skills,parity' "${ID[01]}"
create 16 '16-execute-env-doctor-decisions.md' 'Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and clean doctor-flagged orphan worktrees' chore P2 'agents,stories,tooling' "${ID[01]}"
create 18 '18-verifier-per-language-evidence.md' 'Landing evidence must cover each changed language: verifier runs no TS/Go/rust-fe tests and hides output' task P2 'agents,landing,quality-gates' "${ID[01]}"
create 19 '19-wire-every-test-module.md' 'Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate (≈220 unwired tests today)' task P2 'testing,quality-gates,agents' "${ID[01]}"
create 20 '20-gate-sources-affected-completeness.md' 'Task `sources:` and affected-gates omit real inputs (parity matrix, runtime crate, rust-fe tests/, shatter-llm): add a coverage meta test' task P2 'quality-gates,taskfile,agents,parity' "${ID[01]}"
create 21 '21-completion-checklist-spec-docs.md' 'Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes' task P2 'agents,docs,skills' "${ID[01]}"
create 22 '22-planning-rules-location-and-open-decisions.md' 'Planning rules in CLAUDE.md: plan/spec location + status banner, and check open tracker decisions before planning' task P2 'agents,docs,governance' "${ID[01]}"
create 23 '23-mechanical-parallel-parity-gates.md' 'Replace prose-only parallel-path parity with gates: engine_parity E2E, per-BranchType known-answer fixtures, caller-named closures' task P2 'agents,parity,e2e,testing,quality-gates' "${ID[01]}"
create 24 '24-cli-golden-output-contract-suite.md' 'Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips' task P2 'agents,testing,artifacts,report,quality-gates' "${ID[01]}"
create 25 '25-gate-golangci-lint.md' 'Gate golangci-lint and gofmt in check-static (lint ungated; str-2tyfk and str-qwua7.32 closed on false '\''lint passes'\'')' task P2 'go,quality-gates,agents,shatter-go' "${ID[01]}"
create 26 '26-gate-rustfmt.md' 'Restore rustfmt cleanliness once and gate `cargo fmt --check` (tree drifted again after str-fr1v)' task P2 'rust,quality-gates,agents' "${ID[01]}"
create 27 '27-downstream-coverage-goals-epic.md' 'Track downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07' epic P2 'agents,coverage,downstream,kapow,zolem,pickpackit' "${ID[01]}"
create 28 '28-help-tracker-ids-lint-and-unfiled-ui-items.md' 'Lint tracker IDs out of CLI help and file the unfiled 2026-09-04 UI findings' task P2 'agents,cli,usability,audit' "${ID[01]}"
create 29 '29-divergence-tracking-issue-liveness.md' 'validate-parity: fail when a `tracked` divergence points at a closed or missing issue' task P2 'parity,agents,drift' "${ID[01]}"

# blocked-by dependencies (issue is blocked by the listed one)
[ -z "$DRY" ] && bd dep add "${ID[03]}" --blocked-by "${ID[02]}" || echo "dep 03 blocked-by 02"
[ -z "$DRY" ] && bd dep add "${ID[24]}" --blocked-by "${ID[09]}" || echo "dep 24 blocked-by 09"

# notes appended to existing issues (drafts 17, 30, 31)
note 17 '17-note-qwua7.23-rtk-block.md' str-qwua7.23
note 30 '30-note-qwua7.37-premise.md' str-qwua7.37
note 31 '31-note-qwua7.43-bench.md' str-qwua7.43

# secondary cross-reference notes on existing issues
xref() { if [ -n "$DRY" ]; then echo "bd update $1 --append-notes \"$2\""; else bd update "$1" --append-notes "$2"; fi; }
xref str-qwua7.51 "Audit 2026-09-22: real root cause of Owner=Test is the leaked repo-local [user] section in .git/config (fixture identity from the str-jttrf GIT_DIR leak). Tracked by ${ID[04]}."
xref str-qwua7.28 "Audit 2026-09-22: body fact setup-hooks.sh:41 is stale (reverted by str-mpgg1, 84941b37). Decision tracked by ${ID[08]}."
xref str-qwua7.62 "Audit 2026-09-22: reconciliation items (obsolete .1/.12/.18/.19, str-mpgg1, str-qe9pp, str-uj3y, orphans) tracked by ${ID[10]}."
xref str-qwua7.55 "Audit 2026-09-22: per-language landing evidence and bento-rdtn.4/.6 adoption tracked by ${ID[18]}."
xref str-qwua7.24 "Audit 2026-09-22: add shatter-go/CLAUDE.md:138 ('TS and Rust currently declare outcome only') to the stale-claim list."
xref str-u394l.3 "Audit 2026-09-22: proposal — a drift-patrol PENDING slot older than 60 days should turn FAIL."
xref str-wurp "Audit 2026-09-22: add reverse check (every backticked --flag in SPEC exists in clap) and a changelog-row rule (args.rs/SPEC §2 changed without §8 row fails unless waived)."

echo "Done. Run: bd export -o .beads/issues.jsonl (if still tracked) and commit via land-work."
