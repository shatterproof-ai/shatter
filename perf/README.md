# Shatter Perf Scenarios

This directory defines the fixed scenario corpus used by the external profiling
workflow. The harness reads `scenarios.json` and executes the named scenarios
without modifying Shatter source code.

Each scenario declares:

- `id`: Stable scenario name used in reports and artifact paths.
- `kind`: High-level scenario type (`walkthrough`, `command`, or `go_test`).
- `description`: Human-readable intent.
- `command`: Exact argv vector to execute.
- `workdir`: Repository-relative working directory.
- `env`: Extra environment variables for the run.
- `cache_mode`: `cold` or `warm`.
- `warmups`: Unmeasured warmup runs before collection.
- `iterations`: Measured repetitions.
- `timeout_seconds`: Hard timeout per run.
- `profilers`: Which external profilers are valid for the scenario.
- `language`: Optional frontend language for scenario grouping.
- `sample_ref`: Optional reference into `benchmarks/sample-manifest.json` when
  the scenario is tied to the walkthrough/test sample corpus.
- `timing_target`: Whether the harness should inject structured timing into a
  direct `shatter` command, the walkthrough script, or neither.

The initial corpus focuses on real user-facing paths from the walkthrough plus
one Go-isolated package test that can be profiled with `pprof` without changing
Shatter source.

Recording is opt-in. `scripts/perf_runner.py run` requires an explicit
`--results-dir`, and that directory is expected to be untracked in normal local
use. The harness stores both coarse wall-clock summaries and any structured
timing JSON emitted by Shatter in the same result bundle so downstream tools can
build phase breakdown charts and time-series comparisons from one dataset.

Artifact retention policy:

- Default to untracked output directories for local and CI perf runs.
- The recommended local destination is `.shatter/perf-runs/`.
- Treat `perf/results/` as a legacy path, not the preferred default.
- Only record into a tracked directory when you are intentionally blessing a
  curated baseline snapshot.
- The only recommended tracked location is `benchmarks/baselines/`.

Use `perf` first when you need a consistent whole-command view across the mixed
Rust, Go, and TypeScript stack. Use `pprof` only for scenarios that isolate the
Go frontend and need Go-runtime-specific attribution such as goroutine or test
binary hotspots.

Use `scripts/perf_compare.py` or `task perf-compare` when you need a repeatable
baseline-versus-candidate comparison. The compare tool reads two result
directories, flags scenario-level regressions using absolute and percentage
thresholds, and also reports timing-metric and phase-level regressions when the
artifacts include structured timing summaries.
