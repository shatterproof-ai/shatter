---
slug: ly5bz-superseded
kind: note-to-existing
title: "Close str-ly5bz as superseded: bd sync and the JSONL import are retired (D4)"
priority: P3
type: task
labels: [agents, beads, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-ly5bz
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Close str-ly5bz as superseded: bd sync and the JSONL import are retired (D4)

**Target:** `str-ly5bz` (open, P3, "AGENTS.md: bd sync-once-at-landing
cadence narrows the merged-branch-cleanup JSONL safety window")

**Action:** post the comment below, then run
`bd close str-ly5bz --reason "Superseded by <beads-jsonl-consumers-drop-bd-sync id> (audit 2026-09-22, D4): bd sync no longer exists and the cleanup script stops reading the JSONL."`

## Comment text

**Audit 2026-09-22 note: superseded (maintainer decision D4, 2026-09-23)**

The cadence question here is moot, for two reasons:

- `bd sync` does not exist in the installed bd 1.1.0: `bd sync --help`
  reports `unknown command "sync"`.
- Under D4, shatter stops importing `.beads/issues.jsonl` and syncs through a
  Dolt remote instead.

The committed JSONL has been frozen since 134dd616 (2026-09-07). So the "JSONL
safety window" this issue worried about has grown to more than two weeks.
`scripts/cleanup-merged-remote-branches.sh` is currently protecting five
merged branches whose issues are closed (str-rmcrl, str-vr7vq, str-0z1im,
str-6vl7p, str-8q1b4), because the stale JSONL still lists them as
`in_progress`.

Replacement: <beads-jsonl-consumers-drop-bd-sync>. It removes every `bd sync`
mention and makes the cleanup script read live bd after `bd dolt pull`,
refusing to delete when bd is unreachable. It also makes CI drift-patrol SKIP
instead of reading the JSONL.

Evidence: audit findings prior-03 and agent-repo-06
(`audits/2026-09-22/findings.json`).
