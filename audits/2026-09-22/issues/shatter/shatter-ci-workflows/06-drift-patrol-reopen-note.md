---
slug: drift-patrol-reopen-note
kind: reopen-note
title: "Comment on closed str-u394l.1: the scheduled Drift Patrol has failed 7/7 runs (setup-go points at a nonexistent root go.mod)"
priority: P1
type: note
labels: [ci, drift, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-u394l.1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-u394l.1

Target: **str-u394l.1** (closed). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. This issue was closed (bddf2481) on the evidence of "CI Patrol self-test job -> pass" and the PR's ci.yml run. On pull requests, `drift-patrol.yml` skips the `patrol` job (`if: github.event_name != 'pull_request'`), so that evidence could not show a scheduled patrol running.
>
> The scheduled workflow has never executed the patrol. All 7 scheduled runs, from 2026-08-10 to 2026-09-21, failed in `actions/setup-go` with `The specified go version file at: go.mod does not exist` (for example run 35620815498). The cause is that `drift-patrol.yml:85` sets `go-version-file: go.mod`, and the repo has no root go.mod. It should be `shatter-go/go.mod`, the same bug str-wnyzy fixed in ci.yml only.
>
> The fix, a test that every workflow's file paths exist, the missing tracker-server row in DRIFT-PATROL.md, and the requirement that the close reason cite a real scheduled or dispatched patrol run are tracked in `<id of drift-patrol-workflow-go-mod>`.

(Filer: replace the `<id of ...>` placeholder.)
