---
name: swarm-project
description: Shatter-specific swarm extensions. Adds epic/wave-based scheduling via bd swarm CLI and Shatter quality gates. Delegates generic team/worktree/merge mechanics to the global /swarm skill.
user-invocable: true
---

# Swarm Project: Shatter Extensions

This skill extends the global `/swarm` skill with Shatter-specific behavior.
**Always run `/swarm` first** — this skill adds epic mode and project-specific
quality gates on top.

## Epic Mode

When given an epic ID instead of (or in addition to) individual issue IDs,
use wave-based scheduling instead of flat `bd ready` triage.

### Phase 1 override — Epic Triage

1. Run `bd swarm validate <epic-id>` to analyze the DAG structure
   - Computes ready fronts (waves), detects cycles, inversions, disconnected subgraphs
   - If validation fails (exit code 1), report errors and stop
2. Run `bd swarm create <epic-id>` to register the swarm molecule (skip if
   `bd swarm list` already shows one for this epic)
3. Use wave 0 (first ready front) as batch 1. Later waves are the wait queue.
4. Present wave structure to user for approval, then hand off to `/swarm` Phase 2+

### Phase 4 override — Wave Draining

When wave N is fully merged, run `bd swarm status <epic-id>` to confirm which
issues are now unblocked, then spawn teammates for the next wave's ready issues.

### Phase 6 addition — Epic Close

Run `bd swarm status <epic-id>` one final time to confirm all children are
closed. Close the swarm molecule if all work is done.

## Quality Gates (Phase 5)

These override the generic "run project quality gates" step in global `/swarm`:

1. `/check-all` — full cross-language quality gate (Rust clippy + tests, TS tests, Go tests)
2. `/walkthrough-review` — verify the demo walkthrough covers new functionality
   and output is human-readable. Update walkthrough if needed.
