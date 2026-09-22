# Frontier-ranking benchmark

Measures whether an external ranker for exploration frontiers finds branch
sides faster than the built-in `frontier_score` heuristic. Built for the
Jev evaluation (str-hjrnp) but the harness is ranker-agnostic.

## Arms

| arm | what ranks frontiers | needs |
|---|---|---|
| heuristic | `frontier_score` (baseline) | nothing |
| random | uniform scores, deterministic in (seed, round); the floor | nothing |
| cheating | `ScriptedRanker` primed with the reference run's branch ids; the ceiling | `reference.json` |
| generative | heuristic ranking plus the existing LLM seed oracle through the `anthropic` adapter | `ANTHROPIC_API_KEY` |
| jev | `DecisionFrontierRanker` asking one Jev choice question per round | `TYPESAFE_API_KEY`, or a replay cache in `benchmarks/frontier-ranking/jev-replay/` |

Arms whose credentials are missing are skipped, not failed.

## Running

```bash
task bench-frontier-reference        # once, or after fixture changes; commit reference.json
task bench-frontier ARMS=heuristic,random,cheating
task bench-frontier-report
```

`FIXTURES=<comma-separated ids>` and `RESULTS_DIR=<dir>` narrow or relocate
a run. The manifest lives in `benchmarks/frontier-ranking/manifest.json`:
ten seeds, a fixed-execution regime (60 executions) and a fixed-wall-clock
regime (8 seconds), eight TypeScript fixtures tagged by stratum.

The report exits 1 unless, on median executions-to-cover, cheating beats
heuristic and heuristic beats random. Fix the harness before trusting any
other arm's number.

## Reading the report

- **Executions to cover**: observations until every branch side the
  reference run observed has been seen. Lower is better. Runs that never
  cover within budget are censored at budget + 1 rather than dropped, so an
  arm that only covers the easy fixtures cannot look faster than one that
  covers everything. The paired delta against heuristic with its bootstrap
  interval is the headline; read it next to the cover rate.
- **Side coverage**: fraction of reference sides observed within budget;
  the fallback when a run never covers everything.
- **Pareto table**: side coverage under fixed wall-clock against median
  milliseconds per fixed-execution run. A decision call costs roughly 100 ms
  per round; this is where a ranker can lose on time while winning on
  executions.
- **Per stratum**: expect no gain on `z3-easy`; look at `opaque` and
  `loop-stall`.
- **Calibration**: hit rate should rise with score bin. Flat means the
  probabilities carry no information for this task.

## Caveats

- Ranking affects drilling order, bounded-unroll order and seed-oracle
  polling order only. Next-input selection comes from the worklist and
  the meta strategy, and Z3 negates every branch of every new path
  regardless of order. A ranker therefore has limited leverage; the
  cheating arm shows how much is available before any model is judged.
- Discovery is counted per branch *side* (`branch_id * 2 + taken`) from the
  orchestrator's raw results, because branch-id granularity is satisfied by
  the seed inputs at execution 1 on most fixtures.
- `function_source` reaches the ranker only through `OracleHandle`; the
  runner passes an inert mock seed oracle to supply it.
- Jev responses are cached by request fingerprint, and the fingerprint
  covers the full request (frontier order, depth, stall counts), so a
  replay-only run reproduces a recorded run only when exploration state
  matches exactly. The fixed-execution regime with a fixed seed does; the
  wall-clock regime generally does not, and each miss degrades that round
  to the heuristic. The runner fails the `jev` arm if it never produced a
  score, and warns per run whose `rank_log` is empty. Delete the replay
  directory to re-query live. Cache files hold responses only.
- `discoveries` indices count observations (`observed_executions`);
  `total_executions` also counts frontend-skipped and unsupported
  iterations, so compare curves against `observed_executions`.
- Fixture source leaves the machine for the `generative` and live `jev`
  arms.
