---
slug: qwua7-2-scope-note
kind: note-to-existing
title: "Note on str-qwua7.2: check-fresh was defeated by the str-qwua7.3 root cause; CI item moves to the audit CI guard; define execution evidence per required leaf (mixed/missing/cached/failed), not 'executed list non-empty'"
priority: P1
type: task
labels: [quality-gates, landing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [ci-executed-leaf-guard]
existing_id: str-qwua7.2
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.2: re-scope after the str-qwua7.3 root cause, and tighten the execution-evidence definition

**Target:** open issue `str-qwua7.2` ("Add task check-fresh and make verifier, CI and pre-completion prove gates actually ran", P1; verified open with `bd show` on 2026-09-23).
**Action:** post the comment below and amend the acceptance as shown. Do not file a new issue. `blocked_by` lists `ci-executed-leaf-guard` only so the filer can substitute its real id; the note itself can be posted as soon as that issue is filed.

## Comment text (post verbatim, after substituting ids)

> **Audit 2026-09-22 (findings gates-01, gates-02, tests-ci-06): re-scope and tighten this issue.**
>
> **1. Root cause of the "still up to date after deleting checksums" mystery.** It is recorded on str-qwua7.3: a `meta`-stage test runs `task --list-all --json`, which rewrites `.task/checksum/*` for every task with `sources:`. `check-fresh` as specified here (delete checksums, then call the governed task) would have been defeated the same way, because `meta` runs first and re-poisons the cache. Once str-qwua7.3's fix lands, `check-fresh` is viable again, but it is no longer the main defence: positive per-leaf evidence (below) catches hollow runs whatever their cause.
>
> **2. The CI item moves to <ci-executed-leaf-guard>.** The acceptance checks "`.github/workflows/ci.yml:89` calls `task check-fresh`" and "a live run asserts no `Task "<leaf>" is up to date` line" are replaced by that issue, which runs `task check` once in CI and requires every leaf in an explicit expected set to show positive execution evidence. Remove the CI item from this issue's acceptance. This issue keeps the verifier and `/pre-completion` parts; the `/pre-completion` rows are filed as <verifier-per-language-evidence>.
>
> **3. Replace "exits non-zero when the executed list is empty".** That passes when one required leaf ran and the others were cached, missing from the graph, or never started. Amended acceptance for the verifier:
> - Execution evidence is defined **per required leaf** from an explicit expected-leaf list: `executed` = at least one `task: [<leaf>]` command echo line and no `Task "<leaf>" is up to date` line; `cached` = an up-to-date line; `missing` = neither. The verifier fails unless every required leaf is `executed`. It uses the shared parser from <ci-executed-leaf-guard> (for example `scripts/task_leaf_evidence.py`), not a new regex.
> - The verifier's final JSON line sets the bento-recognized per-check `executed` boolean (bento `land.py` validates it as a boolean on each `selected_checks[]` entry; bento-rdtn.6) to `true` only when every required leaf executed. No new top-level keys are added to Shatter's strict receipt schema (`scripts/gate-receipt.py::RESULT_KEYS`), per the str-35vtk.24 note.
> - A test wired into `meta` runs the verifier against a stub `task` that emits fixture logs for each case and asserts the verdict: all required leaves executed → pass; a mixed log where dependencies are cached but every required leaf executed → pass; one required leaf cached → fail naming it; one required leaf absent → fail naming it; everything cached → fail; a required leaf executed but its command failed → fail on the exit code, with the leaf reported `executed`.
> - Close-time proof: one real landing after the change whose verifier JSON line shows `executed: true`, and one forced hollow run (pre-seeded `.task/checksum`) showing the verifier failing with the cached leaves named. Paste both lines into the close reason.
>
> Coordination: str-35vtk.24 (in progress) makes the verifier run exactly one `task check`; build this on top of that, not on the old trio. The verifier timeout and output-tee amendments are on str-qwua7.55.

## Filer notes

- Substitute the filed ids for `<ci-executed-leaf-guard>` and `<verifier-per-language-evidence>`.
- Add `related` links to str-qwua7.3, str-qwua7.55 and str-35vtk.24 if missing.
- Add the label `audit` if missing. Priority stays P1.
