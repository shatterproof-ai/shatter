# External Profiling Workflow

Shatter's external profiling loop is designed to stay outside the product code.
It uses the checked-in scenario corpus in `perf/scenarios.json`, stores routine
artifacts in an untracked directory, and escalates from broad timing data to
runtime-specific profilers only when a scenario is actually slow or regressed.

## Storage Policy

- Routine local runs should write to an untracked directory such as
  `.shatter/perf-runs/`.
- Scheduled CI runs should upload artifacts from an untracked workspace
  directory rather than committing histories to git.
- `perf/results/` is a legacy location and should not be treated as the
  preferred destination for new runs.
- The only recommended tracked location is `benchmarks/baselines/`, and it is
  reserved for curated baseline snapshots such as release checkpoints, major
  optimization milestones, or intentionally refreshed golden baselines.
- Do not check in rolling perf histories or one-off local runs.

## Weekly Run

Run the stable subset on the same Linux environment each week:

```bash
RESULTS_DIR=.shatter/perf-runs/weekly-$(date +%Y%m%d) npx task perf-ci
```

That command:

- runs the stable scenarios listed in `perf/stable-scenarios.txt`
- writes the latest perf bundle under the chosen `RESULTS_DIR`
- refreshes report artifacts inside that same run directory

Review the generated report and compare the median time for each scenario
against the previous run. A scenario is worth triaging when the median regresses
by roughly 10% or more or when the report flags it as unstable.

## Monthly Review

Once per month:

1. Read the latest weekly reports.
2. Rank the slowest persistent scenarios by median wall time.
3. Pick the top 1-3 hotspots.
4. For each hotspot, decide the next profiler step:
   - `perf stat` for a first counter pass
   - `perf record` for native CPU hotspot attribution
   - `pprof` for Go-only scenarios
5. File follow-up issues using `perf/regression-issue-template.md`.
6. Only copy a run into `benchmarks/baselines/` if you are explicitly creating
   or refreshing a curated baseline snapshot.

## Regression Triage

When a scenario regresses:

1. Re-run the exact scenario locally to confirm it.
2. Check whether the slowdown is cold-cache only, warm-cache only, or both.
3. Run `perf stat` first unless the scenario is already Go-isolated and needs `pprof`.
4. Use `perf record` only after the counter pass still leaves the hotspot unclear.
5. Attach the exact artifact paths to the issue.
6. If the slowdown is confirmed and worth preserving for future comparison,
   promote exactly one representative artifact set into `benchmarks/baselines/`.

## Profiler Choice

- Use `perf` first for walkthrough, `explore`, `scan`, and mixed-runtime runs.
- Use `pprof` for Go-isolated scenarios such as `go-frontend-instrument-tests`.
- Keep profiler output paths in the issue so future runs can compare like-for-like.
