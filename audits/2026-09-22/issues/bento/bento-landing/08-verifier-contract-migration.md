---
slug: verifier-contract-migration
kind: new
title: "Verifier contract drift: warn when verifier payloads lack `executed`, flag outdated manifests, require gate output pass-through"
priority: P2
type: feature
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Verifier contract drift: warn when verifier payloads lack `executed`, flag outdated manifests, require gate output pass-through

## Problem

bento-rdtn.6 added a per-check `executed` flag and a guard that fails when every check was served from cache. By the maintainer's decision on that issue, payloads without the field are accepted silently. So verifiers wired before the contract changed get no protection and no signal that they are out of date. Existing consumer repos are never prompted to regenerate their wrappers.

Shatter's hand-written verifier shows the consequence. It emits only `{name, status}` and runs every gate with `>/dev/null 2>&1`, so even the rdtn.4 persisted log is empty. Shatter's `task` gates also report cached "is up to date" tasks as passes. A cached no-op landing gate therefore looks exactly like a real run.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6) and the shatter audit worktree 56c86168:

- `catalog/skills/land-work/scripts/land-work-run-verifier.py:520-545`: the all-cached guard trips only when `all(c["executed"] is False ...)`. The comment says a payload with no `executed` field "stays fully accepted".
- `catalog/skills/land-work/scripts/land.py:116-121`: prints `[cached]`/`[executed]` only when the flags exist.
- `catalog/skills/wire-land-verifier/scripts/wire-land-verifier.py:853-917`: generated wrappers do emit `executed`. Nothing detects or prompts repos whose wrappers predate this.
- `catalog/skills/wire-land-verifier/` has only `SKILL.md`, `metadata.json` and `scripts/`, with no reference doc. The output pass-through requirement belongs in SKILL.md.
- Shatter `scripts/land_work_verifier.sh` (last changed d01a22db, 2026-08-05): `run_check` runs `"$@" >/dev/null 2>&1` (line 17) and emits `{"name":...,"status":...}` only.
- None of the 11 land.py verify lines observed in shatter transcripts shows `[cached]` or `[executed]`.

## Acceptance criteria

- [ ] When any selected check lacks `executed`, run-verifier adds the warning `checks without execution evidence: <names>` to its payload. land.py prints it under the verify step line (land.py currently ignores run-verifier warnings) and includes it in the final JSON.
- [ ] verifier.json gains `contract_version`, and wire-land-verifier writes the current version. The SessionStart doctor flags a manifest with a missing or older `contract_version` with one line: "land-work verifier predates contract vN; regenerate with wire-land-verifier".
- [ ] wire-land-verifier's SKILL.md and generated wrapper template state that gate stdout and stderr must pass through to the verifier's output (captured into the verifier log), never to `/dev/null`. A test asserts that the generated wrapper contains no `/dev/null` redirection of a gate command.
- [ ] Tests cover: payload without `executed` produces the warning, and land.py shows it; payload with `executed` produces no warning; doctor output for missing, old and current `contract_version`.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Suggested approach

Keep the rdtn.6 decision: a missing field still does not fail the verify step. Add the warning and the doctor nudge as a deprecation path. A later issue can make the field mandatory once consumers have regenerated.

## Out of scope

- Rewriting shatter's verifier: shatter str-qwua7.55 and str-qwua7.2.
- Making a missing `executed` a hard failure.

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.6 and bento-rdtn.4 (closed), `land-py-verifier-log-kept`, shatter str-qwua7.55, str-qwua7.2.

Priority: P2 · Type: feature · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/14, bento-13, agent-repo-11 (context)
