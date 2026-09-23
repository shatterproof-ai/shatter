---
slug: scan-progress-post-hoc
kind: new
title: "Scan 'progress' lines print after the scan finishes; --progress mixes JSON into human stderr; run shows no progress"
priority: P2
type: bug
labels: [cli, ux, progress, scan, run, explore, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scan "progress" lines print after the scan finishes; --progress mixes JSON into human stderr; run shows no progress

## Problem

Closed str-7pkp.5 ("Scan shows live progress") promised live scan progress. What users get today:

- **Default `shatter scan`** prints nothing while it works. After the scan completes it prints `[info] [1/10] <fn> (26.0s elapsed)` through `[10/10] <fn> (26.0s elapsed)` all at once, and every line carries the same total elapsed time.
- **`scan --progress`** is live, but it writes raw `{"type":"progress",...}` JSON objects to stderr interleaved with human `[info]` lines. The result is neither readable by a person nor cleanly parseable by a machine.
- **`shatter run .`** wrote 0 bytes to stderr over a 74 s run. There is no progress of any kind until the final report.
- **`explore`** prints three lines per function (`[progress] starting`, `[progress] completed` and `[batch]`). The counters mean different things: `starting 2/3: compute_stats` is the scheduled index, while `completed 1/3: main` is the first completion. The header claims `(16 parallel worker(s))` for one target on one frontend session.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/scan.rs:1412-1424`: `let elapsed = scan_start.elapsed();` is computed once after the scan returns. Then `if !progress { for (i, fr) in result.function_results.iter().enumerate() { log::info!("[{}/{}] {} ({:.1}s elapsed)", ...) } }` runs. This is the post-hoc loop.
- `shatter-cli/src/commands/scan.rs:1335-1365`: the `--progress` handler `eprintln!`s `ProgressEvent::to_json()` on the same stderr stream that `log::info!` uses.
- `shatter-cli/src/commands/run.rs`: no progress handler is wired. A grep for `progress` finds no emitter.
- `shatter-cli/src/commands/explore.rs:4467-4472`: `"Spawned {} frontend session(s) for {} target(s) ({} parallel worker(s))"` prints `parsed.len()` (target *specs*, so one file with three functions counts as 1) and `effective_workers` (the configured capacity from `resolve_parallelism_with_bounds`, explore.rs:4339). The batch loop at explore.rs:5399-5468 keeps up to `effective_workers` batches in flight drawn from every function of every target, so real concurrency is bounded by the number of runnable functions, not by targets. The header therefore neither reports the function count nor the achievable concurrency.
- `shatter-cli/src/args.rs:961-963`: `--progress` is a bool flag ("Emit progress events to stderr during scan").
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `ts-scan.err` has ten `[i/10] ... (26.0s elapsed)` lines with identical elapsed values.
  - `scan-progress.err` has `[info] Discovered ...` and `[info] Scanning ...` followed by `{"type":"progress","status":"started",...}` lines.
  - `run.err` is 0 bytes.
  - `rust-explore3.err` lines 1-6 show `(16 parallel worker(s))` for 1 target, `starting 2/3` before `starting 1/3`, and `completed 1/3: main`.
- Source findings: audit 2026-09-22 cli-ux-06 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F6.

## Acceptance criteria

- [ ] The post-hoc `[i/N] ... elapsed` loop at `scan.rs:1412-1424` is removed.
- [ ] **Human mode** (default) for `scan`, `run` and `explore`: one line per function completion on stderr, written when that function finishes (not buffered to the end). The line has a labelled counter (`done 3/10`), the function's display name, and that function's own duration. Human mode never writes JSON to stderr.
- [ ] **Machine mode** is the existing `--progress` bool flag (no new flag spelling; `--progress` gains the same meaning on `run` and `explore`, and its help says it switches stderr to JSON lines). While it is on, *every* stderr line is one JSON object:
  - progress events keep the existing `{"type":"progress",...}` shape;
  - every log record at warn or error level (including the sandbox refusal/warning from sandbox-backend-disables-guard and the runtime-crate error from rust-runtime-path-and-doctor) is `{"type":"log","level":"warn"|"error","message":"<the human text>"}`;
  - info-level human lines are suppressed.
  This contract is written into SPEC (CLI output section) with a §8 changelog row.
- [ ] `shatter run` emits the same human and machine progress as `scan`.
- [ ] The explore header states three separately labelled numbers: frontend sessions, runnable functions, and maximum concurrent batches, where the last is `min(effective_workers, runnable functions)` and is labelled as a limit ("up to N concurrent"). A unit test covers 1 target / 3 functions / 16 workers (expects "3 functions", "up to 3 concurrent"). If the implementer finds the scheduler can run two batches of one function at once, the header reports `effective_workers` labelled as configured capacity instead, and the close reason says why.
- [ ] Explore's `starting` and `completed` counters are labelled so they cannot be read as one sequence (for example `started #2 of 3` versus `done 1/3`).
- [ ] **Live-progress regression test** (CLI integration test, must fail on current main and pass after the fix). The fixture has two functions: `fast` returns immediately; `gated` busy-waits until a release file named by an env var exists (with a 30 s safety cap). Run `scan` with `--workers 2` on piped stderr and read it incrementally. Assert that the `fast` completion line is read **while the child process is still running and before `gated` has completed**; only then create the release file and let the run finish. On current main default scan prints nothing until the end, so the test times out waiting for the `fast` line. A test that only compares "stderr before stdout" ordering does not satisfy this criterion, because the current post-hoc loop already passes it.
- [ ] Machine-mode test: `scan --progress` and `run --progress` over a directory that includes a target triggering a warning (for example a TS target under a backend-only sandbox setting, or a missing-frontend skip) — every stderr line parses as JSON, and the warning appears as a `type: log` object. Fails on current main (human `[info]` lines interleave).
- [ ] Walkthrough and gauntlet pass (`task walkthrough`, `task gauntlet`), and `/walkthrough-review` has been run on the new stderr shape. Record the result.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Define one progress sink trait in `shatter-cli`, with `HumanSink` (TTY-aware, one line per completion) and `JsonSink` (JSON lines only, plus a `log` layer that converts warn/error records), fed by the orchestrator's existing `ScanProgressUpdate` callbacks. Wire the same sink into explore, scan and run so the three commands cannot drift again. This is the parallel-parity rule in CLAUDE.md, applied to progress.

## Out of scope

- Report formatting and content (see run-report-verdict-and-coverage-metrics and markdown-drops-render-plain-info).
- Progress bars and spinners. Plain lines are enough.

## Related

str-7pkp.5 (closed; the comment on it is scan-progress-reopen-note), str-7pkp.1, str-poyv, str-4oa1 (earlier scan-progress bugs, closed).

## Priority / Type

P2, bug (a regression of a closed feature).
