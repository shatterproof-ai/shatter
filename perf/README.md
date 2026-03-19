# Shatter External Profiling Scenarios

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

The initial corpus focuses on real user-facing paths from the walkthrough plus
one Go-isolated package test that can be profiled with `pprof` without changing
Shatter source.

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
