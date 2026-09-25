# Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason)

- Priority: P3
- Type: feature
- Labels: report,ux,explore
- Tracker action: new issue
- Related: str-zt4v, str-qwua7.15. Also L1 finding cli-ux-02: `explore --format text|html` is ignored on stdout and `--render`/`--format` overlap. The fix should land together with the flag unification.
- Source findings: audit 2026-09-22 cli-ux-14 (confirmed; P3)

<!-- body -->
## Problem
`explore --render plain` (deprecated) shows `Branches: 3/3`, `[random: 3 (100%)]` and `Symbolic: 3/3 constraints`. The default markdown shows only the path count and line coverage. Neither shows the termination reason, although `stop_reason` exists in the artifact JSON. The walkthrough-review skill (`.claude/skills/walkthrough-review/SKILL.md`, items 2, 4 and 7) lists branch coverage, discovery method and termination reason as essential.

## Acceptance criteria
- The markdown report includes branch coverage (`Branches x/y`), discovery-method breakdown and a `stopped: <reason>` line per function.
- `--render plain` can be removed without losing information.
- Golden test on `ts/01-arithmetic.ts:classifyNumber` asserting these lines.

## Scope
In: markdown content. Out: per-path constraint column (needs SymExpr pretty-printing; follow-up).
