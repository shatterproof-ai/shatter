# Planning guidance: an experiment or benchmark plan must start with a falsification probe

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P3
- labels: documentation
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: sessions-12

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

On 2026-09-21/22 a Shatter session went from brainstorming to writing-plans to executing-plans, and landed a 4-issue epic (str-hjrnp.1-.4: a FrontierRanker trait, a DecisionOracle plus Jev adapter in shatter-llm, a bench runner and a report script; about 11 commits on main). Afterwards the agent concluded "even a ranker that knows the answer can't beat the heuristic at the current hook points". A cheap oracle or upper-bound run with a scripted perfect ranker would have shown this before any new public trait or crate dependency landed. The negative result had value, but the infrastructure cost was avoidable.

## Acceptance criteria

- [ ] The dotfiles planning guidance (the leaf covering plans, or the inlined core rules) states: for experiments or benchmarks, Task 1 of the plan is a falsification or upper-bound probe on existing code. New public traits, dependencies or landed infrastructure wait until the probe shows a measurable effect.
- [ ] The guidance says experiment code stays on an experiment branch or expedition until then.

## Source

Shatter audit 2026-09-22 finding sessions-12 (session `c1689435`).
