# Swarm Project Config — Shatter

## Quality Gates

Full quality gate on main after all merges: `/check-all`

## Pre-Completion Skill

Teammates MUST run `/pre-completion` before reporting done. Reject any completion
message that lacks the summary table or shows FAIL status.

## Post-Merge Validation

Run `/walkthrough-review` to verify the demo walkthrough:
- Covers any new functionality added by the issues
- Output is human-readable per the walkthrough criteria
- If the walkthrough needs updates, make them

## Epic Mode

When working a structured epic, use `bd swarm validate <epic-id>` for DAG analysis
and wave-based scheduling. Use `bd swarm status <epic-id>` to monitor progress
between waves. This is preferred over flat `bd ready` mode for any work with
dependencies between issues.
