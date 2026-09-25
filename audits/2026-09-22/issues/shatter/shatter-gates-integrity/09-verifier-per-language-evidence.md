---
slug: verifier-per-language-evidence
kind: new
title: "/pre-completion must prove per-language gate execution: add a changed-crate -> required-leaf evidence row, a property-test row, and an 'up to date means not run' rule"
priority: P2
type: task
labels: [quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [ci-executed-leaf-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# /pre-completion must prove per-language gate execution

## Problem

`/pre-completion` (`.claude/skills/pre-completion/SKILL.md`) tells agents to run `task affected` and copy its `Gates selected:` list. It never asks whether the leaves covering each changed crate or language actually **executed**, and it never says that `Task "X" is up to date` means the leaf did not run. With Task checksum caching (and the str-qwua7.3 poisoning), a completion report can list the right gates while none of the relevant tests ran. The skill also has no property-test row, although CLAUDE.md Completion Checklist item 2 requires one. Sessions since 2026-09-04 saw 65 `task ... is up to date` results in 24 sessions (finding agent-repo-11), and agents treated them as passes.

Scope, reconciled against open issues (checked with `bd show` on 2026-09-23):
- The **landing verifier** (`scripts/land_work_verifier.sh`, `verifier.json`) is not changed here. str-35vtk.24 (in progress, P1) replaces its trio with exactly one `task check` and writes receipts. str-qwua7.55 (open, P2) owns verifier honesty and the timeout. str-qwua7.2 (open, P1) owns executed-vs-cached reporting for the verifier. The audit's verifier findings are added to those issues as notes (`qwua7-55-verifier-timeout-note`, `qwua7-2-scope-note`), not re-filed.
- This issue owns only the `/pre-completion` skill rows. It reuses the per-leaf evidence parser built by `ci-executed-leaf-guard` rather than writing another regex.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `.claude/skills/pre-completion/SKILL.md` Phase 2 runs `task affected` and records `Gates selected:` only. A grep for `property`, `PBT` or `up to date` finds nothing.
- `task affected` selects gates per path (`scripts/affected-gates.py`), but a selected gate can be served from `.task/checksum` without running.
- CLAUDE.md "Completion Checklist" item 2 requires property-test adequacy for new or modified public functions.

## Acceptance criteria

- [ ] Phase 2 of `/pre-completion` tees `task affected` output to a log and runs the `ci-executed-leaf-guard` evidence script on it with the required-leaf set derived from the diff. The mapping lives in one checked-in table (shared with or derived from `scripts/affected-gates.py`, not duplicated prose): shatter-core → core:test (plus core:test-ignored when the selector adds it), shatter-cli → cli:test, shatter-ts → ts:test, shatter-go → go:test, shatter-rust → rust-fe:test, shatter-rust-runtime → rust-rt:test + rust-fe:test, shatter-llm → its llm test leaf once it exists (str-35vtk.36).
- [ ] The skill states that a required leaf reported `cached` or `missing` is **not** evidence. The agent re-runs that leaf directly (`task --force <leaf>`; `--force` on `task affected` does not propagate to nested leaves) and records the leaf's executed result, or reports the gap. The output table has one row per required leaf with its `executed`/`cached`/`missing` verdict.
- [ ] The skill gains a property-test row that restates CLAUDE.md Completion Checklist item 2 and asks for the names of the property tests covering each new or modified public function (or an explicit "none needed: <reason>").
- [ ] A `meta` test asserts the mapping table covers every crate directory that `scripts/affected-gates.py` classifies, so a new crate cannot be added without a required-leaf entry.
- [ ] Proof: in a scratch worktree (after the str-qwua7.3 fix), commit a one-line comment change to a `shatter-go` file, run `task go:test` once (it executes and warms the cache), then follow the updated skill's Phase 2. Paste the row showing `go:test` as `cached`, then the directed `task --force go:test` re-run showing `executed`. Record both in the close reason. Running the same procedure against today's skill text produces no per-leaf row at all; note that as the before-state.

## Suggested approach

Add the steps to Phase 2 and a row to the output table. Put the crate → leaf table next to the evidence script from `ci-executed-leaf-guard`, so CI, `/pre-completion` and later the verifier (str-qwua7.2) read one definition.

## Out of scope

- Any change to `scripts/land_work_verifier.sh` or `verifier.json` (str-35vtk.24, str-qwua7.55, str-qwua7.2; see the companion notes in this bucket).
- The root-cause fix for meta-stage checksum poisoning (str-qwua7.3).
- bento-side verifier changes (bento-rdtn.4 and bento-rdtn.6 have shipped).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: quality-gates, agents, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: ci-executed-leaf-guard (for the shared evidence script).
- Related: str-qwua7.2, str-qwua7.55, str-35vtk.24, str-35vtk.36, `qwua7-55-verifier-timeout-note`, `qwua7-2-scope-note`.
- Source findings: tests-ci-06 (pre-completion half; the verifier half moved to the two notes), agent-repo-11. Draft shatter-agent/18.
