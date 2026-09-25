---
slug: falsification-probe-first
kind: new
title: "Planning guidance: experiment and benchmark plans must start with a falsification or upper-bound probe"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Planning guidance: experiment and benchmark plans must start with a falsification or upper-bound probe

Part of #<epic>. Priority: P3. The verifier lowered this from P2: it is a process preference, not a defect, and the negative result it produced had value. Type: enhancement.

## Problem

`~/dotfiles/docs/agent-guidance/planning.md` is 9 lines long. It covers plan-then-approve, re-planning and verification steps. It says nothing about experiments. As a result, an agent can plan and land new public infrastructure for an experiment before it has checked that the experiment can show any effect.

## Evidence

Shatter session `c1689435-5dae-4c52-8fc4-930ec4e87246`, from 2026-09-21 20:19 to 2026-09-22 02:14:

- The session went from brainstorming to writing-plans to executing-plans. It then created and landed a 4-issue epic, str-hjrnp.1 to .4: a FrontierRanker trait, a DecisionOracle plus Jev adapter in shatter-llm, a bench runner and a report script. That is 11 commits on main in `4c4aca4e..85a08ddd`; the verifier counted 11, not the 16 the finding claimed.
- Afterwards the agent concluded: "even a ranker that knows the answer can't beat the heuristic at the current hook points".
- About the 60-execution budget, the agent said: "It's a number I chose, and it's arguably the weakest part of the benchmark… Ranking only influences drilling and bounded unroll, which fire when a frontier has stalled".
- The user asked: "er... explain further why jev cannot improve on the exploration frontier."

A cheap upper-bound run with a scripted perfect ranker on the existing code would have shown the ceiling before a new public trait and a crate dependency landed on main.

## Acceptance criteria

- [ ] `docs/agent-guidance/planning.md` states two rules for plans whose goal is an experiment, benchmark or "does X improve Y" question:
  - Task 1 is a falsification or upper-bound probe on the existing code, for example an oracle, a scripted perfect choice, or an ablation.
  - New public traits, new dependencies and landed infrastructure wait until the probe shows a measurable effect.
- [ ] It says that experiment code stays on an experiment branch or a bento expedition until the probe passes, and that a negative probe result is reported as the outcome.
- [ ] Proof at close: the closing comment shows the output of `grep -n "falsification\|upper-bound" ~/dotfiles/docs/agent-guidance/planning.md`.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Re-evaluating the landed str-hjrnp work in shatter.

## Dependencies

None.

## Source

Shatter audit 2026-09-22 finding sessions-12 (verifier: partially confirmed, lowered to P3).
