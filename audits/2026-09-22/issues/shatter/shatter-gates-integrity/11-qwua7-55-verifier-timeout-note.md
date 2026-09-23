---
slug: qwua7-55-verifier-timeout-note
kind: note-to-existing
title: "Note on str-qwua7.55: a `timeout` in verifier.json is not read by bento's land-work; enforce the verifier timeout in a way the installed runner honours, and keep one exact `task check` per str-35vtk.24"
priority: P2
type: task
labels: [landing, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [verifier-per-language-evidence]
existing_id: str-qwua7.55
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.55: the timeout criterion as written cannot be met by editing verifier.json

**Target:** open issue `str-qwua7.55` ("Repo-owned landing fixes now (verifier honesty, timeout); orchestration driver comes from bento", P2; verified open with `bd show` on 2026-09-23).
**Action:** post the comment below and amend the acceptance as shown. Do not file a new issue. This replaces the one-line companion note that the earlier draft of `verifier-per-language-evidence` proposed.

## Comment text (post verbatim)

> **Audit 2026-09-22 (findings tests-ci-06, agent-repo-11): amendments to this issue's repo-owned verifier items.**
>
> **1. "verifier.json gains a timeout" would enforce nothing.** In the installed bento (2.3.88 inspected on 2026-09-23; the Codex cross-check found the same in 2.3.73), `skills/land-work/scripts/land-work-run-verifier.py` takes its timeout only from the `--timeout` CLI argument (`:208`, applied at `:344-383` with a process-group kill), and `land.py` forwards its own `--timeout` (`:77`, `:303-304`). Neither reads a timeout from `.agent-plugins/bento/bento/land-work/verifier.json`, which today holds only `schema_version` and `command`. A new field there would be ignored.
>
> **Amended acceptance for the timeout item** (replaces "verifier.json gains a timeout"):
> - The timeout is enforced by a mechanism the runner actually honours. Either (a) `scripts/land_work_verifier.sh` bounds its own gate run (for example `timeout --kill-after=30 <seconds> task check`, killing the whole process group, with the seconds passed as an argument in `verifier.json`'s `command` array, e.g. `["scripts/land_work_verifier.sh", "--timeout", "3600"]`, and a documented default when absent; no new environment variable) and reports `status: "failed"` with a timeout reason in its final JSON line; or (b) the repo's landing instructions (AGENTS.md and the landing section the agents follow) pass `--timeout <seconds>` to bento's `land.py`/`land-work-run-verifier.py`. If a manifest-level timeout is wanted instead, that is a bento feature request, not a repo-owned fix.
> - A runtime test, wired into `meta`, runs the verifier (or the exact wrapper used in option b) against a stub `task` on `PATH` that sleeps past a short timeout. It asserts the run ends within the timeout plus the kill grace period, no stub process survives, and the reported status is a timeout/failure. It fails on today's script.
>
> **2. What the verifier runs.** str-35vtk.24 (in progress) already replaces the `test-standard`/`parity`/`conformance` trio with exactly one exact-candidate `task check`. This issue's "runs task check (or states exactly what it runs)" item should be satisfied by .24's change plus correcting the header comment at `scripts/land_work_verifier.sh:3` ("Runs the same gates ci.yml uses") and any CLAUDE.md/AGENTS.md text that repeats it. Running `task affected` in the verifier is **not** an option; it would conflict with .24.
>
> **3. Output.** `run_check` (`scripts/land_work_verifier.sh:14-23`) runs each gate as `"$@" >/dev/null 2>&1`, so bento's persisted verifier log is empty on failure or kill. str-35vtk.24's notes already require the replacement to stop discarding output. Whichever of .24/.55 lands first must include a fixture where the stub gate prints stdout/stderr sentinels and exits non-zero, and assert both sentinels reach the verifier's output.
>
> Executed-vs-cached reporting for the verifier stays with str-qwua7.2 (see the companion note there). The `/pre-completion` per-language evidence rows are filed as <verifier-per-language-evidence>.

## Filer notes

- Replace `<verifier-per-language-evidence>` with that issue's filed id.
- Add `related` links to str-35vtk.24 and str-qwua7.2 if missing.
- Add the label `audit` if missing.
- `blocked_by` lists `verifier-per-language-evidence` only so it is filed first and its id can be substituted; the note can be posted immediately after.
