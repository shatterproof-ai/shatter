---
slug: qwua7-28-superseded
kind: note-to-existing
title: "Close str-qwua7.28 as superseded: JSONL import retired (D4), no BEADS_HOOK_TIMEOUT change"
priority: P2
type: task
labels: [agents, beads, git-hooks, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.28
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Close str-qwua7.28 as superseded: JSONL import retired (D4), no BEADS_HOOK_TIMEOUT change

**Target:** `str-qwua7.28` (open, P2, "Install the BEADS_HOOK_TIMEOUT env
section setup-hooks.sh already defines (raise to 60 s), ...")

**Action:** post the comment below, then run
`bd close str-qwua7.28 --reason "Superseded by <beads-retire-jsonl-import-dolt-remote id> (audit 2026-09-22, maintainer decision D4): the post-checkout stall is the JSONL import, not the timeout value."`
The filer substitutes the real id for the slug placeholder.

## Comment text

**Audit 2026-09-22 note: superseded (maintainer decision D4, 2026-09-23)**

This issue will not be implemented. The maintainer has decided not to change
`BEADS_HOOK_TIMEOUT` and not to add any hook env block.

Measured 2026-09-23: a direct, unwrapped `bd -v hooks run post-checkout`
spent about 6 minutes in "importing JSONL from .beads/issues.jsonl" (the file
has 1,733 records) while using about 10 s of CPU. Hook-wrapped runs are cut
off by the 300 s timeout (land.py `create_preview` 234.9-301.2 s on 11
landings). What the import waits on is being pinned down by
<beads-hook-stall-diagnosis>. The imported file is a stale
export, last committed at 134dd616 on 2026-09-07, and bd 1.1.0 itself calls
it "an export, not cross-machine sync or source of truth". Tuning the timeout
only changes how much of that wait is cut off. It does not stop a stale
snapshot from being imported into a newer database.

Replacement work:
- <beads-jsonl-import-clobber-check>: checks whether the import has
  overwritten newer DB state, and repairs any damage.
- <beads-hook-stall-diagnosis>: finds what the hook waits on, lists every
  import entry point, and records one latency baseline.
- <beads-retire-jsonl-import-dolt-remote>: stops the import on every entry
  point through bd configuration, makes the Dolt remote the sync channel, and
  measures `git worktree add` and land.py `create_preview` against the
  baseline. This issue's post-checkout smoke-test idea lives there.
- <beads-jsonl-consumers-drop-bd-sync>: docs, skills, CI and the cleanup
  script.

Stale fact in this issue's body: it says `scripts/setup-hooks.sh:41` already
defines the BEADS_HOOK_TIMEOUT section. That has been false since b5cd25ec
(str-mpgg1, merged in 84941b37 on 2026-09-02).
`grep -rn BEADS_HOOK_TIMEOUT scripts/ .beads/hooks Taskfile.yml` finds
nothing. The only occurrence is the beads-managed default
`${BEADS_HOOK_TIMEOUT:-300}` in `.git/hooks/post-checkout` and `pre-push`.

Evidence: audit findings sessions-05 and agent-repo-07
(`audits/2026-09-22/findings.json`).
