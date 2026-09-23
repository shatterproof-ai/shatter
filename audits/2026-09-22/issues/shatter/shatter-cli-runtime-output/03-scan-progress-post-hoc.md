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
- `shatter-cli/src/commands/explore.rs:4468`: `"Spawned {} frontend session(s) for {} target(s) ({} parallel worker(s))"` prints the configured worker count, not min(targets, workers).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `ts-scan.err` has ten `[i/10] ... (26.0s elapsed)` lines with identical elapsed values.
  - `scan-progress.err` has `[info] Discovered ...` and `[info] Scanning ...` followed by `{"type":"progress","status":"started",...}` lines.
  - `run.err` is 0 bytes.
  - `rust-explore3.err` lines 1-6 show `(16 parallel worker(s))` for 1 target, `starting 2/3` before `starting 1/3`, and `completed 1/3: main`.
- Source findings: audit 2026-09-22 cli-ux-06 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F6.

## Acceptance criteria

- [ ] The post-hoc `[i/N] ... elapsed` loop in `scan.rs` is removed.
- [ ] Default human mode for `scan`, `run` and `explore`: one live line per function completion on stderr, printed as each function finishes. The line has a labelled counter (`done 3/10`), the function's display name, and its own duration or the real wall-clock elapsed at completion.
- [ ] Machine mode (`--progress` or `--progress json`, one spelling documented) writes only JSON lines to stderr, one object per line, and every line parses as JSON. Human `[info]` log lines are suppressed or moved to a different sink while it is on. Human mode never emits JSON.
- [ ] `shatter run` emits the same progress as `scan`.
- [ ] The explore header reports the workers actually used (min(targets, workers)), and its starting and completed counters are labelled so they cannot be read as the same sequence.
- [ ] A CLI integration test drives a 3-function scan (a stub or fast TS fixture) and asserts that (a) each per-function progress line reaches stderr before the final report is written to stdout, with timestamps or ordering captured from the child's pipes, and (b) under `--progress json` every stderr line parses as JSON. The test must fail on current main and pass after the fix. Record both runs in the close reason.
- [ ] Walkthrough and gauntlet pass (`task walkthrough`, `task gauntlet`), and `/walkthrough-review` has been run on the new stderr shape. Record the result.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Define one progress sink trait in `shatter-cli`, with `HumanSink` (TTY-aware, one line per completion) and `JsonSink` (JSON lines only), fed by the orchestrator's existing `ScanProgressUpdate` callbacks. Wire the same sink into explore, scan and run so the three commands cannot drift again. This is the parallel-parity rule in CLAUDE.md, applied to progress. When the JSON sink is active, route `log::info!` output away from stderr or suppress it at info level.

## Out of scope

- Report formatting and content (see run-report-verdict-and-coverage-metrics and markdown-drops-render-plain-info).
- Progress bars and spinners. Plain lines are enough.

## Related

str-7pkp.5 (closed; the comment on it is scan-progress-reopen-note), str-7pkp.1, str-poyv, str-4oa1 (earlier scan-progress bugs, closed).

## Priority / Type

P2, bug (a regression of a closed feature).
