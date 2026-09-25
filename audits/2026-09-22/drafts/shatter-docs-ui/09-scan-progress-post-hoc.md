# Scan "progress" lines are printed after the scan finishes; --progress mixes JSON into human stderr; `run` shows no progress

- Priority: P2
- Type: bug
- Labels: cli,ux,progress,scan,run
- Tracker action: new issue (regression of closed str-7pkp.5 "Scan shows live progress")
- Related: str-7pkp.5, str-7pkp.1, str-poyv, str-4oa1
- Source findings: audit 2026-09-22 cli-ux-06 (confirmed)

<!-- body -->
## Problem
Default `shatter scan` prints `[info] [1/10] <fn> (26.0s elapsed)` through `[10/10] … (26.0s elapsed)` only after the scan completes, all with the same elapsed time. During the scan the user sees nothing. `scan --progress` interleaves `{"type":"progress",…}` JSON lines with `[info]` human lines on stderr. `shatter run .` wrote 0 bytes to stderr over 74 s. Explore prints three lines per function, its "starting 2/3" and "completed 1/3" counters count different things, and it reports "(16 parallel worker(s))" for a single target.

## Current code facts
- `shatter-cli/src/commands/scan.rs:1413-1424` loops over `result.function_results` after the scan returns and logs `[i/N] name (elapsed)` with a single shared elapsed value.
- `shatter-cli/src/commands/run.rs` emits no progress.

## Acceptance criteria
- On a TTY, scan and run print live per-function completion lines on stderr as functions finish, with a per-function or real elapsed time.
- `--progress json` (or the existing `--progress`) emits only JSON lines on stderr, with no interleaved human text. Human mode emits no JSON.
- The post-hoc loop is removed.
- Explore's worker count reflects the actual workers used (min(targets, workers)), and its counters are labelled consistently.
- A CLI test drives a 3-function scan with a stub frontend and asserts that the progress lines arrive before the final report and that `--progress json` stderr is pure JSON lines.

## Suggested approach
Share one progress renderer (a trait with human and JSON sinks) across explore, scan and run, fed by the orchestrator's completion events.

## Scope
In: progress output for explore, scan and run. Out: report formatting.
