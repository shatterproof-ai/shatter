---
slug: qwua7-14-reverify-on-main
kind: reopen-note
title: "Reopen str-qwua7.14: its 'not reproducible' closure cited e50fc399, a stray fixture commit that is not on origin/main; re-verify on an origin/main build"
priority: P1
type: bug
labels: [agents, git, frontend-rust, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.14
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen str-qwua7.14: re-verify on an origin/main build

Target: **str-qwua7.14** (CLOSED, P1 bug, "Rust frontend: walkthrough
examples 0% covered — analyzer/harness param-type disagreement
(hypothesis)"). Action: `bd reopen str-qwua7.14`, then
`bd comments add str-qwua7.14` with the text below. Keep its priority. Split
out of `fixture-corruption-incident-reverify` so the re-diagnosis has its own
owner and proof.

## Comment text

> Audit 2026-09-22 (findings agent-repo-16, prior-06, prior-09;
> `audits/2026-09-22/findings.json`). Reopened because the closure's
> reference point is not on main.
>
> - The close reason says "Not reproducible against current main
>   (e50fc399)" and "rebuilt shatter-cli/shatter-rust from HEAD".
>   `git merge-base --is-ancestor e50fc399 origin/main` exits **1**:
>   `e50fc399` ("init", author `Test <test@example.com>`, 2026-09-07,
>   82 files, -12,090 lines) is a stray commit made by the GIT_DIR fixture
>   leak (str-jttrf / str-y0rcz). The diagnosis therefore **may** have run on a
>   corrupted tree; the "not reproducible" result is unproven, not refuted.
>   Incident record: <fixture-corruption-incident-reverify>.
> - The close reason's other points (the cited walkthrough evidence was
>   mis-cited; `negotiate_language`'s 5% is a separate tractability gap) are
>   not disputed.
>
> **Acceptance for the re-verification:**
> 1. Choose a SHA `S` with `git merge-base --is-ancestor S origin/main` exit
>    0 (record the command and exit code). Work in a scratch linked
>    worktree, never the primary checkout.
> 2. Rebuild the CLI and the Rust frontend at `S` before running anything
>    (a stale binary produced a false audit finding before; prior-09), and
>    record `shatter --version` or the binary's build SHA.
> 3. Run the walkthrough's Rust step for `classify_number` and `safe_divide`
>    through the same harness mode the walkthrough uses (standalone-file vs
>    crate), and record the command, the coverage numbers and any
>    `deserialization failed` errors.
> 4. Decide from that output: if the 0%-coverage / deserialization error
>    reproduces, keep the issue open as a confirmed bug with the repro
>    command; if not, close it with a reason citing `S`, the commands and
>    their output. Either way the reason follows the SHA-labelling rule
>    proposed on str-qwua7.51 (<qwua7-51-identity-root-cause>).
