---
slug: rapid-failfile-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.4: rapid failfile purge incomplete"
priority: P3
type: note
labels: [go, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.4: rapid failfile purge incomplete

Target: **str-qwua7.4** (closed 2026-09-22 at 16794cef, "Fix TestPlanParam_HTTPRequestBodyInvariants (mined literal vs generic seed) and purge rapid failfiles"). Do not reopen it. Post the comment below, which points to the new issue.

## Comment text

> Audit 2026-09-22 (findings tests-ci-15, prior-23): the "purge rapid failfiles" half of this issue was only done for `planner/`.
>
> - A March rapid failfile is still tracked on main:
>   `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/TestPropertyExecTimeoutAlwaysPositive-20260306134644-2197485.fail` (added in f0a58576, 2026-03-06). `git ls-tree -r --name-only origin/main | grep '\.fail$'` still lists it at 70465921.
> - `shatter-go/.gitignore` ignores only `planner/testdata/rapid/**/*.fail`. The acceptance text asked for `testdata/rapid/**/*.fail` to be removed from git and ignored, i.e. the whole class.
>
> The close reason lists the gates that ran but does not re-check this acceptance bullet. The remaining work is tracked in **<rapid-failfile-purge id>** ("Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide"). The TestPlanParam fix in this issue is not affected.

(Filer: replace `<rapid-failfile-purge id>` with the id assigned to slug `rapid-failfile-purge`.)
