# Bundle: shatter-cli-runtime-output

- **Bucket:** shatter-cli-runtime-output
- **Repo:** shatter (bd in /home/ketan/project/shatter, prefix str)
- **Parent epic:** Epic: Audit 2026-09-22 findings
- **Theme:** What users see while and after running: sandbox guard bypass, scan progress, runtime-path errors and doctor readiness, per-language outcome rendering, run report verdict and coverage metrics.
- **Status:** drafts only, revised after the Codex cross-check (see REVISION.md). Nothing is filed (D6).
- **Code re-verified against:** audit worktree HEAD 56c86168. Stale line numbers from the old drafts are corrected: helpers.rs hint 497 -> 418-424; scan.rs post-hoc loop 1413-1424 -> 1412-1424.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to take it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue, blocked by both, re-decides the positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and sync the tracker through a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT or hook-bypass guidance.
- **D5 Git identity:** the leaked [user] section is already removed. Add a .mailmap (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>), a git-state check (identity override / example.com / core.bare / hooksPath), and a fixture .git/config snapshot test.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

None of D1-D6 changes this bucket's content directly. Its issues are CLI and runtime output fixes.

## Contents

| NN | Slug | Kind | Target | P | Title |
|---|---|---|---|---|---|
| 01 | sandbox-backend-disables-guard | new | - | P1 | SHATTER_SANDBOX_BACKEND turns off the host-write guard for TS and Rust targets, and the docs recommend it |
| 02 | docs-first-run-reopen-note | reopen-note | str-qwua7.8 | P1 | Comment on closed str-qwua7.8: sandbox remedy disables write protection; SPEC changelog backfill claims edits never made |
| 03 | scan-progress-post-hoc | new | - | P2 | Scan 'progress' lines print after the scan finishes; --progress mixes JSON into human stderr; run shows no progress |
| 04 | scan-progress-reopen-note | reopen-note | str-7pkp.5 | P2 | Comment on closed str-7pkp.5: scan progress now prints after the scan ends |
| 05 | rust-runtime-path-and-doctor | new | - | P2 | Rust explore outside the source tree fails per function on the undocumented SHATTER_RUNTIME_PATH, with the language labelled `any` |
| 06 | per-language-outcome-rendering | new | - | P2 | Error outcomes render inconsistently across languages; outcome values are byte-truncated mid-token and can panic on non-ASCII |
| 07 | run-report-verdict-and-coverage-metrics | new | - | P2 | Run report opens with an unexplained 'degraded' verdict, in internal field names, before its H1 |
| 08 | markdown-drops-render-plain-info | new | - | P3 | Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason) |
| 09 | doctor-rust-runtime-note | note-to-existing | str-qwua7.40 | P2 | Note on str-qwua7.40: also report the shatter-rust-runtime crate location in doctor's Rust section |
| 10 | rust-hint-once-note | note-to-existing | str-qwua7.13 | P1 | Note on str-qwua7.13: the missing-Rust-frontend hint is still printed twice in explore and is contributor-oriented |
| 11 | doctor-execution-readiness | new | - | P2 | `shatter doctor` does not check node/go toolchains or sandbox/host-write readiness, so it reports all green on a machine where every execution command refuses to run |
| 12 | rust-main-default-exclusion | new | - | P3 | Rust frontend offers `fn main` as an explore/scan target; Go already excludes its `main` entrypoint |
| 13 | rust-mocks-to-string-diagnosis | new | - | P3 | Diagnose why the Rust explore report lists `Mocks: to_string` for safe_divide |
| 14 | run-validity-degraded-cause-diagnosis | new | - | P2 | Diagnose why `shatter run` rates an all-TS, fully supported example directory as 'degraded' (61.5% represented source) |
| 15 | coverage-headline-metric-unification | new | - | P2 | explore, scan and run headline three different coverage metrics without naming them; make line coverage the named headline everywhere |

---

<!-- file: 01-sandbox-backend-disables-guard.md -->

---
slug: sandbox-backend-disables-guard
kind: new
title: "SHATTER_SANDBOX_BACKEND turns off the host-write guard for TS and Rust targets, and the docs recommend it"
priority: P1
type: bug
labels: [sandbox, safety, docs, cli, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SHATTER_SANDBOX_BACKEND turns off the host-write guard for TS and Rust targets, and the docs recommend it

## Problem

When there is no sandbox, Shatter refuses to execute targets (str-gg9v). The documented remedy for that refusal is to set `SHATTER_SANDBOX_BACKEND=docker|bwrap`, and the docs call it the recommended one. Only the Go frontend reads this variable and implements a backend.

The CLI treats *any* non-empty value other than `none` as proof of confinement, for every frontend. When the variable is set it:

- passes the default-deny gate, and
- skips the throwaway-directory `IsolationGuard`, so `SHATTER_HOST_WRITE_DIR` is never exported.

For TypeScript and Rust targets this removes all write protection. Target code writes straight into the invoking directory. That is the incident str-gg9v was created to prevent, and the recommended remedy brings it back.

A secondary gap: the CLI accepts values the Go runner rejects, such as a typo like `dcoker`. The Go runner errors with `sandbox: unsupported backend`, but the CLI still counts the value as a sandbox and drops the guard.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/host_writes.rs:55-63` `sandbox_backend_configured()` returns true for any trimmed, non-empty value other than `none`.
- `shatter-cli/src/host_writes.rs:77-79` `execution_permitted()` passes the gate when that function returns true.
- `shatter-cli/src/host_writes.rs:142-145` `setup()` returns `Ok(None)` (no `IsolationGuard`) whenever a backend is "configured". The comment says "The OS sandbox already contains the target's writes."
- The only reader of the variable in the Rust, Go and TS sources is `shatter-go/sandbox/runner.go:16` (`EnvironmentBackendKey`). `runner.go:118` rejects unknown values.
- The TS and Rust frontends redirect relative writes only through `SHATTER_HOST_WRITE_DIR` (`shatter-rust/src/executor.rs:1045-1060`, and the TS executor, see `shatter-ts/src/executor.test.ts:753`). The CLI never sets that variable when a backend is set.
- Docs that present the variable as the recommended remedy. Only `README.md:309` carries a Go-only note, and it is a parenthetical on a line labelled Recommended; every other site has none:
  - `README.md:309-310`: "# Recommended: run targets inside an OS sandbox (Go frontend)." followed by `export SHATTER_SANDBOX_BACKEND=docker`. It does not say that TS and Rust targets lose write protection.
  - `README.md:321-322` and `README.md:326-329` tell CI and wrapper users to prefer `SHATTER_SANDBOX_BACKEND`.
  - `QUICKSTART.md:83-85` recommends it for the TS example.
  - `SPEC.md:606-609` and `SPEC.md:622-624` say a configured backend "satisfies both controls at once".
  - `refusal_message()` at `shatter-cli/src/host_writes.rs:98-115` says "Configure an OS sandbox (recommended)".
  - The `--allow-host-writes` help at `shatter-cli/src/args.rs:170-171` calls the backend "the safer alternative".
  - `shatter-go/CLAUDE.md:257` says "the CLI never sets `SHATTER_HOST_WRITE_DIR` in that case". The fix below makes that sentence false.
- Repro from the audit, on main 9516036d. A TS target `touch()` calls `fs.writeFileSync('marker-' + s + '.txt', ...)`:
  - `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch --max-iterations 5` exits 0 and leaves `marker-long.txt` and `marker-short.txt` in the cwd.
  - The same run with `--allow-host-writes` instead leaves the cwd clean.
- Source findings: audit 2026-09-22 docs-01 (verdict confirmed, P1). Area evidence: `audits/2026-09-22/areas/docs.md`.

## Execution policy (decided in this draft; the maintainer can override before filing)

`SHATTER_SANDBOX_BACKEND` confines only Go targets. For TS and Rust targets it is **not** an opt-in to execution. This is fail-closed, the same as the str-gg9v default-deny: a user who asked for an OS sandbox and did not get one should be refused, not silently downgraded to unsandboxed execution.

| Opt-ins present | Go targets | TS / Rust targets |
|---|---|---|
| backend only | run under the backend | **refused** (default-deny) with a message that the backend is Go-only and names `--allow-host-writes` / `SHATTER_ALLOW_HOST_WRITES=1` |
| backend + `--allow-host-writes` (or `SHATTER_ALLOW_HOST_WRITES=1`) | run under the backend | run unsandboxed in the throwaway directory (`IsolationGuard`, `SHATTER_HOST_WRITE_DIR` exported) |
| `--allow-host-writes` only | run in the throwaway directory | run in the throwaway directory |

In a mixed-language `scan`/`run` with the backend only, the Go targets run and the TS/Rust targets are reported as skipped with the refusal reason (same status mechanism as other skipped targets); the command does not abort the Go work.

## Acceptance criteria

- [ ] The table above is implemented and recorded in SPEC §2.10, with a §8 changelog row for the behavior change.
- [ ] Whenever any TS or Rust target is executed, the `IsolationGuard` exists and `SHATTER_HOST_WRITE_DIR` is exported to that frontend, whether or not a backend is set. The early `return Ok(None)` at `host_writes.rs:142-145` is gone.
- [ ] A backend value other than `none`, `bwrap` or `docker` (after trimming, case as Go's `runner.go` accepts it) is rejected before any frontend is spawned, with an error naming the accepted values. It is never treated as a sandbox. Unit test covers `dcoker`, empty, `none`, `bwrap`, `docker`.
- [ ] CLI integration tests, each run from a fresh temp cwd with a target that writes a relative marker file **and** returns a value that proves it ran:
  - TS and Rust, `--allow-host-writes`: exit 0, the report shows at least one executed path for the target (paths > 0), the marker file exists inside the throwaway directory (the test locates it via a test-only hook or by pointing `SHATTER_HOST_WRITE_DIR`'s parent at a test-owned dir) and the cwd is byte-for-byte unchanged. A refused or failed run fails this test.
  - TS and Rust, backend only (`SHATTER_SANDBOX_BACKEND=docker`, no allow flag): the target is refused, the stderr names the Go-only rule, zero executions happen, and the cwd is unchanged. On current main this case exits 0 and leaves `marker-*.txt` in the cwd, so it **fails on current main and passes after the fix**.
  - TS and Rust, backend + `--allow-host-writes`: same assertions as the allow-only case (execution proven, cwd clean). Fails on current main (marker in cwd).
  - Mixed directory (TS + Go files), backend only, with a stub Go backend or a logged skip when neither docker nor bwrap exists: the TS target is reported as skipped with the refusal reason; the Go target is attempted.
  - Record the red run on main and the green run on the branch (test names and output excerpts) in the close reason.
- [ ] Warning and refusal text: in human mode each is one plain line on stderr. In machine mode (`--progress`) they follow the machine-mode stderr contract that scan-progress-post-hoc defines (a JSON log object whose `message` is the same text). If scan-progress-post-hoc has not landed, the human line is used in both modes.
- [ ] Docs state that OS sandbox backends are Go-only and that TS and Rust need `--allow-host-writes` or `SHATTER_ALLOW_HOST_WRITES=1`: README ("Executing Target Functions Safely" and the CI/wrapper paragraphs at 321-329), QUICKSTART 83-85, SPEC §2.10 (606-624), the `refusal_message()` text, the `--allow-host-writes` help, and `shatter-go/CLAUDE.md:257` (the "CLI never sets `SHATTER_HOST_WRITE_DIR`" sentence).
- [ ] `protocol/parity-matrix.yaml` records sandbox-backend support per frontend (Go yes, TS no, Rust no), and `task parity` passes.
- [ ] `task affected` passes, with its `Gates selected` output recorded in the close reason.

## Suggested approach

Make the "is this contained?" decision per frontend, not per process:

1. Always create the `IsolationGuard` for execution commands that will run a TS or Rust target, and export `SHATTER_HOST_WRITE_DIR`. Go already ignores it when its sandbox is enabled (`Runner.Enabled()`), so no Go change is needed.
2. Replace the process-wide `execution_permitted()` with a per-language check: Go passes with a valid backend or the allow opt-in; TS and Rust pass only with the allow opt-in.
3. Validate the backend value in `sandbox_backend_configured()` against the set Go accepts.
4. Drive the per-frontend rule from data: a `sandbox_backends` capability row in `protocol/parity-matrix.yaml`, so a future TS or Rust backend is a matrix change plus implementation, not a hidden CLI assumption.

## Out of scope

- Implementing OS sandbox backends for TS or Rust.
- Redirecting relative writes under `--allow-host-writes` for TS paths not yet covered. That is str-joyqu (open).
- The SPEC changelog backfill errors from str-qwua7.8. That is spec-changelog-backfill.

## Related

str-gg9v (original default-deny), str-02i70 (per-frontend throwaway-dir redirect), str-joyqu, str-qwua7.8 (closed; its addendum criterion required the docs never to treat the Go-only variable as proof of confinement), sa-oio (shatter-agents note that the backend is Go-only). The comment on str-qwua7.8 is docs-first-run-reopen-note. In this bucket, doctor-execution-readiness reports this rule and scan-progress-post-hoc defines the machine-mode stderr contract.

## Priority / Type

P1, bug. This is a safety regression: the documented recommended remedy lets target code write into the user's repository.

---

<!-- file: 02-docs-first-run-reopen-note.md -->

---
slug: docs-first-run-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.8: sandbox remedy disables write protection; SPEC changelog backfill claims edits never made"
priority: P1
type: note
labels: [docs, sandbox, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.8
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.8

Target: `str-qwua7.8` (closed 2026-09-14 with the bare reason "Closed"). Action: add the comment below. Do not reopen. The remaining work is tracked in the two new issues it names.

## Comment text

> Audit 2026-09-22: two of this issue's acceptance criteria are unmet on main, although the issue is closed.
>
> 1. **Sandbox remedy (docs-01, P1).** The addendum required that the TS example "never treat the Go-only backend variable as proof of confinement". README.md:309-310 and 321-329, QUICKSTART.md:83-85, SPEC.md:606-609 and 622-624, the `refusal_message()` text (shatter-cli/src/host_writes.rs:98-115) and the `--allow-host-writes` help (shatter-cli/src/args.rs:170-171) all still present `SHATTER_SANDBOX_BACKEND` as the recommended remedy. Only README.md:309 notes "(Go frontend)", as a parenthetical on the Recommended line; none of them says that TS and Rust targets get no confinement from it. It is worse than a docs gap: `host_writes.rs:142-145` skips the throwaway-directory IsolationGuard whenever the variable is set, so a TS or Rust target that writes a relative path writes straight into the cwd. This was reproduced with `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch`, which left `marker-*.txt` in the cwd. Tracked in the new issue **sandbox-backend-disables-guard** (<filed id>).
> 2. **SPEC changelog backfill (docs-05, P2).** The §8 rows added here claim section updates that were never made. Row 2026-08-10 / str-1fwt claims §2.8 and §2.9, but §2.8 has no `.gitignore` block or implicit-init text, and §2.9 doctor still describes only embed staleness. Row 2026-07-18 / str-mktn claims §3.6, which never mentions `shatter.config.json`. The header `Last updated: 2026-09-09` predates the SPEC commits of 09-14 (21981b1d, 8bd5a667, c8bceb32) and 09-20 (2de05fd9, str-nfg4y), and those have no rows. Tracked in the new issue **spec-changelog-backfill** (<filed id>).
>
> Process note: this closed with no close reason listing verified acceptance items. Requiring one is str-qwua7.51.

## Filing note

The filer script must replace `<filed id>` with the ids assigned to sandbox-backend-disables-guard (this bucket, 01) and spec-changelog-backfill (bucket shatter-docs).

---

<!-- file: 03-scan-progress-post-hoc.md -->

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

---

<!-- file: 04-scan-progress-reopen-note.md -->

---
slug: scan-progress-reopen-note
kind: reopen-note
title: "Comment on closed str-7pkp.5: scan progress now prints after the scan ends"
priority: P2
type: note
labels: [cli, progress, scan, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-7pkp.5
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-7pkp.5

Target: `str-7pkp.5` ("Scan shows live progress", closed). Action: add the comment below. Do not reopen. The fix is tracked in the new issue it names.

## Comment text

> Audit 2026-09-22: this behavior has regressed. Default `shatter scan` no longer shows live progress. `shatter-cli/src/commands/scan.rs:1412-1424` computes `scan_start.elapsed()` once after the scan returns, then loops over `result.function_results` and logs `[i/N] <fn> (<elapsed>s elapsed)`. All N lines appear together after the scan, with the same elapsed value (for example ten lines of `(26.0s elapsed)`). Nothing is printed while the scan runs. `scan --progress` is live but interleaves raw JSON objects with human `[info]` lines on stderr. `shatter run` prints no progress at all (0 bytes of stderr over 74 s). Evidence: `audits/2026-09-22/cli-ux-transcripts/ts-scan.err`, `scan-progress.err` and `run.err`, and finding cli-ux-06. Tracked in the new issue **scan-progress-post-hoc** (<filed id>), which also adds a test asserting that one function's completion line reaches stderr while another function is still running (a stderr-before-stdout ordering check would not catch this regression).

## Filing note

The filer script must replace `<filed id>` with the id assigned to scan-progress-post-hoc (this bucket, 03).

---

<!-- file: 05-rust-runtime-path-and-doctor.md -->

---
slug: rust-runtime-path-and-doctor
kind: new
title: "Rust explore outside the source tree fails per function on the undocumented SHATTER_RUNTIME_PATH, with the language labelled `any`"
priority: P2
type: bug
labels: [rust-frontend, docs, ux, install, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust explore outside the source tree fails per function on the undocumented SHATTER_RUNTIME_PATH, with the language labelled `any`

## Problem

A user who installs `shatter-rust` on PATH (as the missing-frontend hint tells them to) and explores a `.rs` target outside the shatter source tree gets:

1. `execute error (FileNotFound): cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`, **once per function**.
2. A "Failure impact" table whose only row is labelled `any`, with no `rust` row, although every target is `.rs`.
3. No documentation of `SHATTER_RUNTIME_PATH`: it is not in README, QUICKSTART, SPEC or any `--help` text, and the error does not say what value to set or where the CLI looked.

Scope note: this draft was narrowed during the cross-check. The doctor and missing-frontend-hint parts of the original finding are owned elsewhere:

- Rust-frontend resolution and runtime-crate location in `shatter doctor`: str-qwua7.40 (see doctor-rust-runtime-note in this bucket).
- The missing-frontend hint printed twice and its length: str-qwua7.13 (see rust-hint-once-note in this bucket).
- Toolchain and sandbox/host-write readiness in `shatter doctor`: doctor-execution-readiness (this bucket).

## Evidence

Re-verified against the audit worktree (code at HEAD 56c86168):

- `shatter-rust/src/executor.rs:1198-1223` `find_runtime_crate_path()`: reads `SHATTER_RUNTIME_PATH` first (used only if `<path>/Cargo.toml` exists), then walks up to five ancestors of the **`shatter-rust` executable** (`std::env::current_exe()`), not the cwd, looking for a sibling `shatter-rust-runtime/Cargo.toml`. Otherwise it returns `"cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH"`. So a `shatter-rust` built inside a checkout finds the crate from any cwd, and a copied or installed binary never does.
- `shatter-cli/src/commands/build_frontend.rs:607-630` is the only other place that mentions the variable (warnings during `build-frontend`).
- `grep -n SHATTER_RUNTIME_PATH README.md QUICKSTART.md SPEC.md` returns no matches.
- `shatter-cli/src/commands/explore.rs:3240-3278`: the failure-impact rollup has per-language classifier rows only for Go (`BuildFailed`) and TS (`BuildFailed`/`RuntimeFailed`). Rust failures only reach the outcome-only `("any", tok)` row.
- Transcript `audits/2026-09-22/cli-ux-transcripts/rust-explore2.out` / `.err`: the runtime-crate error three times (three functions), and the failure-impact row `| runtime_failed | any | 3 | 1 | 47 | 47 | 100.0% |`.
- Why tests miss it: the E2E Rust suite does not rely on discovery at all. `shatter-core/tests/e2e_concolic_rust.rs:122-125` and `shatter-core/tests/support/rust_frontend_harness.rs:77` set `SHATTER_RUNTIME_PATH` explicitly, and the test binaries live under the workspace `target/`, where the executable-ancestor walk would also succeed. No test runs a relocated `shatter-rust` with the variable unset.
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F10.

## Acceptance criteria

- [ ] When the runtime crate cannot be located, the error is reported **once per run** (not once per function) and the remaining Rust functions in that run are reported as skipped for the same reason. The message names `SHATTER_RUNTIME_PATH`, gives an example value (`/path/to/shatter/shatter-rust-runtime`), and lists where it looked (the env var value if set but invalid, and the executable-ancestor walk).
- [ ] The failure-impact table has a `rust` row for Rust runtime-setup failures (a dedicated category such as `runtime_crate_missing`, added alongside the Go/TS classifiers at explore.rs:3244-3258). The `any` rollup row may remain.
- [ ] In machine mode (`--progress`) the once-per-run error follows the machine-mode stderr contract from scan-progress-post-hoc if that has landed; otherwise it is the same human line.
- [ ] `SHATTER_RUNTIME_PATH` is documented in the README install section and in the env-var table that str-qwua7.20.2 adds (if it exists by then), until str-qwua7.60 embeds the runtime.
- [ ] **Relocated-frontend integration test** (must fail on current main and pass after the fix): build or copy `shatter-rust` into a temp directory with no `shatter-rust-runtime` sibling in any of its five ancestors, put that directory first on PATH, **remove `SHATTER_RUNTIME_PATH` from the child environment**, run `shatter explore` from a temp cwd outside the repository on a `.rs` fixture with **at least three functions**, and assert: exactly one occurrence of the runtime-crate error on stderr, the message contains `SHATTER_RUNTIME_PATH`, and the failure-impact table has a `rust` row. On current main the error appears three times and there is no `rust` row.
- [ ] Positive control in the same test setup: with `SHATTER_RUNTIME_PATH` set to the real crate, explore of the same fixture completes with at least one executed path per function. This proves the documented remedy works.
- [ ] Record the red run on main and the green run on the branch in the close reason. `cargo test --test e2e_concolic_rust` passes. SPEC §8 has a changelog row. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Make the runtime-crate check a per-run precondition on the Rust frontend session (check once when the session starts, or cache the first `FileNotFound` for the run) instead of a per-execute failure. Carry the language on the failure summary so the rollup can add a Rust row.

## Out of scope

- Embedding the Rust frontend or runtime in the shatter binary (str-qwua7.60).
- Doctor checks (str-qwua7.40, doctor-execution-readiness).
- The missing-frontend hint text and its duplication (str-qwua7.13).
- The general SHATTER_* env-var table (str-qwua7.20.2). This issue only adds the `SHATTER_RUNTIME_PATH` row, if that table exists by then.

## Related

str-qwua7.40 (open, doctor Rust section), str-qwua7.13 (open, hint once / scan-run agreement), str-qwua7.20.2, str-qwua7.60. In this bucket: doctor-rust-runtime-note, rust-hint-once-note, doctor-execution-readiness, scan-progress-post-hoc.

## Priority / Type

P2, bug.

---

<!-- file: 06-per-language-outcome-rendering.md -->

---
slug: per-language-outcome-rendering
kind: new
title: "Error outcomes render inconsistently across languages; outcome values are byte-truncated mid-token and can panic on non-ASCII"
priority: P2
type: bug
labels: [report, ux, parity, rust-frontend, go-frontend, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Error outcomes render inconsistently across languages; outcome values are byte-truncated mid-token and can panic on non-ASCII

## Problem

The same logical outcome, "division by zero", from the three `04-errors` examples renders three different ways in the default (markdown) explore report:

- **TS:** ``throws `Error: division by zero` ``. This is correct.
- **Go:** ``throws `function_error: division by zero` ``. Go functions return errors rather than throw, and `function_error` is an internal outcome category name.
- **Rust:** ``returns `{"Err":"division by zero"}` ``. Successful struct results render as ``returns `{"Ok":{"avg":2.0,"flag":null,"max":2....` ``, a raw serde JSON envelope cut off mid-token.
- Rust sections end with `- *Mocks: to_string*` as a stray list item directly after the table.

The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`, "3. What is the outcome of each behavior?" and criterion "D. Error paths describe the error type or value") asks for the error value in languages where errors are values.

Scope note: this draft was split during the cross-check. Excluding Rust `main` from default targets is rust-main-default-exclusion, and checking whether `to_string` really is mocked is rust-mocks-to-string-diagnosis. This issue covers rendering only.

## Evidence

Re-verified against the audit worktree (code at HEAD 56c86168):

- The default markdown explore output is rendered by `shatter-cli/src/render.rs`: `shatter-cli/src/commands/explore.rs:3620-3638` dispatches `OutputFormat::Md` to `render::explore_fn_view` / `render::render_explore_fn`. `shatter-core/src/explorer.rs:2950` `format_exploration_report` is the legacy plain/ANSI path (`--render plain`, explore.rs:3639-3650), and `format_exploration_report_verbose` (explorer.rs:3183) is the trace-level path.
- `shatter-cli/src/render.rs:124-130`: the outcome is `throws `{thrown_error}`` if `thrown_error` is set, else `returns `{value_short(return_value)}``. There is no per-language formatting; Go's `function_error:` prefix arrives inside `thrown_error`.
- `shatter-cli/src/render.rs:277-284` `value_short()` does `let s = v.to_string(); if s.len() > 40 { format!("{}...", &s[..37]) }`, truncating by **byte** count. That causes the mid-token cut. From reading the code (not run), `&s[..37]` also panics when byte 37 is not a UTF-8 character boundary, for example a string outcome containing non-ASCII text. `shatter-core/src/explorer.rs:3262` has the same `&s[..37]` pattern.
- `shatter-cli/src/render.rs:139-142` pushes `Mocks: ...` into `extras`, which the template renders as a trailing list item.
- **Why a JSON-shape heuristic is unsafe for Rust `Result`:** the Rust harness serializes ordinary return values with serde, so `Result::Err("x")` and a `HashMap` containing only the key `"Err"` produce the same JSON. The analyzer maps `Result<T, E>` to a generic `TypeInfo::Union { variants, enum_values: [] }` (`shatter-rust/src/analyzer.rs:1325-1331`), which does not distinguish `Result` from other unions. The executor does know the declared return type is a `Result` (`is_result_return_shape`, `shatter-rust/src/executor.rs:2266-2275`), but nothing carries that to the CLI.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `ts-safedivide--concolic.out` (TS form), `go-explore.out` (``throws `function_error: division by zero` ``), `rust-explore3.out` lines 1-19 (the `{"Err":...}` and truncated `{"Ok":...` rows and the `- *Mocks: to_string*` line).
- The examples are `ts/04-errors.ts`, `go/04-errors.go` and `rust/04_errors.rs` in the examples checkout (`SHATTER_EXAMPLES_DIR`).
- Source findings: audit 2026-09-22 cli-ux-11 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F11.

## Acceptance criteria

- [ ] One outcome display function, used by `render.rs` (markdown) and by both core formatters (`format_exploration_report`, `format_exploration_report_verbose`):
  - TS exceptions: ``throws `Error: <msg>` `` (unchanged).
  - Go error returns: ``returns error `<msg>` ``. The internal `function_error` prefix never appears in rendered output.
  - Rust: ``returns `Err(<msg>)` `` and ``returns `Ok(<v>)` `` **only when trustworthy metadata says the declared return type is a `Result`**. That metadata comes from the Rust frontend (for example a result-variant field on the execute response, set from the executor's existing `is_result_return_shape` knowledge) — never from the JSON shape of the value. Without that metadata the value renders as plain JSON (elided correctly).
- [ ] If the Rust metadata is a new execute-response field, it is protocol-visible: update `protocol/` schema and regenerated bindings, `protocol/parity-matrix.yaml`, `shatter-rust/CLAUDE.md`, and run `task parity` and `task conformance`. If no protocol change is made, record why in the close reason.
- [ ] Negative tests: a Rust function returning a `HashMap<String, String>` with the single key `"Err"`, and a struct with a single field named `Ok`, render as JSON values, not as `Err(...)` / `Ok(...)`. A Go function whose return value is a string starting with `function_error:` (not an error) is not rewritten.
- [ ] One `elide(s, max_chars)` helper replaces both `&s[..37]` sites. A proptest over arbitrary strings, including multibyte ones, shows it never panics, always returns valid UTF-8, never exceeds the limit plus the ellipsis marker, and ends with the marker whenever it shortened the input. A unit test with a non-ASCII string whose byte 37 is mid-character panics on current `value_short` (red) and passes after the fix (green); record both in the close reason.
- [ ] Cross-language golden test renders the three `04-errors` examples through the markdown path and asserts the division-by-zero wording for each language and one success case for Rust (`Ok(...)` not cut mid-token).
- [ ] The `Mocks:` line, when present, renders as a labelled line under the section heading (before the table), not as a list item after it. Golden test covers it.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Put the display function in core, keyed on `(language, outcome kind, value, frontend-supplied result-variant)`. For `elide`, cut on `char_indices` and prefer the last JSON token boundary before the limit.

## Out of scope

- Excluding Rust `main` from default targets (rust-main-default-exclusion).
- Whether `to_string` is really mocked for `safe_divide` (rust-mocks-to-string-diagnosis).
- Changing protocol outcome categories.
- The per-path constraint column (see markdown-drops-render-plain-info).

## Related

In this bucket: rust-main-default-exclusion, rust-mocks-to-string-diagnosis, markdown-drops-render-plain-info.

## Priority / Type

P2, bug.

---

<!-- file: 07-run-report-verdict-and-coverage-metrics.md -->

---
slug: run-report-verdict-and-coverage-metrics
kind: new
title: "Run report opens with an unexplained 'degraded' verdict, in internal field names, before its H1"
priority: P2
type: bug
labels: [report, ux, run, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Run report opens with an unexplained 'degraded' verdict, in internal field names, before its H1

## Problem

`shatter run .` over a directory of six TS files, all in supported languages, begins its stdout with `## Report Validity: degraded` and a one-row table whose detail reads `represented_source_percent=61.5 below high threshold 75.0`. Only after that does `# Shatter Run Report` appear. The reader sees a verdict, then the title. The verdict uses an internal field name and threshold, and does not say which source is unrepresented or why, in a directory where every file is supported. The recommended action, "Inspect unrepresented_*_lines buckets", names JSON fields the markdown report does not show.

Scope note: this draft was split during the cross-check (the slug is kept for stability; the metric part moved out). Why the audit's all-TS repro was classified degraded is run-validity-degraded-cause-diagnosis; the three commands headlining three different coverage metrics is coverage-headline-metric-unification. This issue covers the placement and wording of the verdict only.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/run.rs:673-674` renders `render_validity_markdown(...)` and prints it with `print_markdown` *before* `print_summary_report(...)` at `run.rs:679`, which writes the H1.
- `render_validity_markdown` at `shatter-cli/src/commands/run.rs:1989-2010` writes `## Report Validity: {label}` and then a raw Reason/Detail/Recommended-action table.
- The degraded reason text is at `shatter-cli/src/commands/run.rs:1706-1715`: `represented_source_percent={rep_pct:.1} below high threshold {HIGH_REPRESENTATION_PCT:.1}` with "Inspect unrepresented_*_lines buckets ...".
- The H1 `# Shatter Run Report` is written at `shatter-cli/src/commands/scan.rs:2034` (the shared summary writer).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `run.out` has a blank line 1, `## Report Validity: degraded` on line 2, the reason row on line 6, and `# Shatter Run Report` on line 8.
- Source findings: audit 2026-09-22 cli-ux-13 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13. This was also 2026-09-04 usability-ui item 16, which was never filed.
- Existing tests for the block: `shatter-cli/src/commands/run.rs:3484-3513` (`render_validity_markdown_emits_verdict_and_reasons`, `..._high_run_collapses_to_no_issues`, `..._escapes_pipe_in_detail`).

## Acceptance criteria

- [ ] The `run` markdown report's first non-blank line is its H1 (`# Shatter Run Report`). The validity verdict follows directly under it.
- [ ] The verdict is one plain-language sentence per reason, built at render time from the reason and the run summary (if stored on `ValidityReason`, it is `#[serde(skip)]`; the machine `code`/`detail`/`recommended_action` stay unchanged in JSON). Example: "Validity: degraded. 38% of source lines are in functions that were not explored (3 failed, 2 unsupported); see Unrepresented source below." The sentence names counts per bucket and the affected files or functions (up to a small limit, then "and N more").
- [ ] The markdown never contains `represented_source_percent`, `unrepresented_*_lines` or other JSON field names. A test asserts this for the degraded, low and high tiers. If the recommended action points the reader at a section, that section exists in the markdown report.
- [ ] JSON output (`report_validity`, `validity_reasons`) is byte-identical before and after for the same run summary (unit test on the serializer), so machine consumers are unaffected.
- [ ] Snapshot test of the top of the `run` markdown report for a fixture summary with one degraded reason: H1 first, then the verdict sentence. The H1-first assertion fails on current main and passes after the fix; record both in the close reason. The existing tests at `run.rs:3484-3513` are updated to the new shape.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Move the validity block into the summary writer, after the H1 (`scan.rs:2034` is the shared writer), instead of printing it from `run.rs` before calling `print_summary_report`.

## Out of scope

- Why the all-TS repro is degraded (run-validity-degraded-cause-diagnosis).
- Coverage metric naming and unification across commands (coverage-headline-metric-unification).
- The scan report's double H1, absolute paths in function columns, and "Interesting Inputs" curation (scan-report-headline-and-paths, bucket shatter-reports-and-specs).
- Changing the validity thresholds.
- Progress output (scan-progress-post-hoc).

## Related

str-jeen.5 (closed; introduced the `report_validity` layer), str-4ad5, scan-report-headline-and-paths. In this bucket: run-validity-degraded-cause-diagnosis, coverage-headline-metric-unification.

## Priority / Type

P2, bug.

---

<!-- file: 08-markdown-drops-render-plain-info.md -->

---
slug: markdown-drops-render-plain-info
kind: new
title: "Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason)"
priority: P3
type: feature
labels: [report, ux, explore, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason)

## Problem

`explore --render plain` is marked "(deprecated)" in every help page, yet it shows more than the default markdown report:

- `Branches: 3/3 (100%)`
- `[random: 3 (100%)]`, the discovery-method breakdown
- `Symbolic: 3/3 constraints (100%)`

The default markdown report shows only the path count and line coverage. Neither mode says why exploration stopped (worklist exhausted, iteration budget or timeout), although `stop_reason` is in the artifact JSON. The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`, "7. Exploration completeness" and criterion "J. Completeness signal") asks the report to say whether exploration was complete, which the missing termination reason and branch figure leave unanswered. More generally, the default renderer shows less than the deprecated one, so `--render plain` cannot be removed without losing information.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/args.rs:41` documents `plain` as "Legacy plain ANSI text output (deprecated)".
- The plain renderer's lines come from `shatter-core/src/coverage_metrics.rs:269-277` (`Branches: {covered}/{total} ...`) and `:328` (`Symbolic: {}/{} constraints ...`).
- The default markdown path is `shatter-cli/src/render.rs` (`shatter-cli/src/commands/explore.rs:3620-3638` dispatches `OutputFormat::Md` to `render::explore_fn_view`/`render_explore_fn`). It emits neither the branch nor the discovery lines, and never references `stop_reason`.
- The plain path is `shatter-core/src/explorer.rs:2950` `format_exploration_report` (explore.rs:3639-3650), which calls `coverage_metrics::format_coverage_metrics`. Besides the three lines above it conditionally prints: the MC/DC block (`coverage_metrics.rs`), a stubbed-import warning (`stubbed_modules`), float-probe results, abandoned frontiers, opaque-type suggestions with a config hint, perf lines (`--perf`), and GA stats. Some of these may already have markdown equivalents; nobody has inventoried them.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `render-plain.out` has the `Branches:`, `[random: ...]` and `Symbolic:` lines, and `render-md-color.out` has 0 `Branches` lines.
- Source findings: audit 2026-09-22 cli-ux-14 (confirmed; the verifier lowered it to P3 because this is report richness, not wrong output). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F14. The termination reason was also 2026-09-04 usability-ui item 12, which was never filed.

## Acceptance criteria

- [ ] For each explored function, the default markdown explore report includes:
  - branch coverage as a named secondary line (`Branch coverage: x/y (z%)`, consistent with coverage-headline-metric-unification if that lands first)
  - the discovery-method breakdown (for example `found by: random 3, Z3 0`)
  - a `Stopped: <reason>` line mapped from `stop_reason` to plain words (worklist exhausted / iteration budget reached / time limit), and `Stopped: unknown` only when `stop_reason` is absent
- [ ] Golden test on `ts/01-arithmetic.ts:classifyNumber` asserts those three lines. It fails on current main (no `Branch` or `Stopped` line in markdown).
- [ ] The mapping is an exhaustive `match` on `StopReason` (`shatter-core/src/explorer.rs:529`, set by `classify_stop_reason` at :1999), so a new variant cannot render raw, and a unit test covers each variant's wording.
- [ ] **Parity inventory, not removal.** The close reason contains a table of every fact the plain renderer can print (at least: paths/lines summary, branches, discovery breakdown, symbolic constraints, MC/DC, stubbed-import warning, float probes, abandoned frontiers, opaque suggestions, perf, GA stats) with, for each, either "in markdown" plus the test that covers it, or "not in markdown" plus a reason. This issue does **not** remove `--render plain` and does not claim markdown is complete beyond the three required lines; removal is decided in explore-format-flag-ignored using this inventory.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Reuse the `coverage_metrics.rs` computations and add markdown variants next to the ANSI ones rather than recomputing. Land this before explore-format-flag-ignored (the `--render`/`--format` unification, bucket shatter-cli-flags-and-help) removes or hides `--render plain`, so that decision can use the inventory.

## Out of scope

- A per-path input-constraint column. It needs SymExpr pretty-printing; file it as a follow-up if wanted.
- Turning the entire walkthrough-review rubric into a gate.
- Removing `--render plain` (explore-format-flag-ignored).

## Related

str-zt4v, str-qwua7.15 (covered only where `--render` appears in help), explore-format-flag-ignored (cli-ux-02), str-qwua7.10 (walkthrough gate checks exit codes only).

## Priority / Type

P3, feature.

---

<!-- file: 09-doctor-rust-runtime-note.md -->

---
slug: doctor-rust-runtime-note
kind: note-to-existing
title: "Note on str-qwua7.40: also report the shatter-rust-runtime crate location in doctor's Rust section"
priority: P2
type: note
labels: [rust-frontend, doctor, install, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.40
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.40

Target: `str-qwua7.40` (open, P2, "`shatter doctor`: report whether shatter-rust is resolvable, from where, and its version/protocol match"). Verified open with `bd show` on 2026-09-23. Action: add the comment below. str-qwua7.40 keeps ownership of doctor's Rust section, and all of its existing acceptance checks stand (resolved path and how it was found, `frontend_version`/`protocol_version` from a handshake with a mismatch warning, warning-only exit unless Rust is required, same resolver as scan/explore, fake-binary unit test).

## Comment text

> Audit 2026-09-22 (cli-ux-10) adds one check to this issue's Rust section. A resolvable `shatter-rust` is not enough: an installed or copied binary fails every Rust execution with `cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`, because `find_runtime_crate_path()` (`shatter-rust/src/executor.rs:1198-1223`) only honours `SHATTER_RUNTIME_PATH` or a `shatter-rust-runtime/` sibling within five ancestors of the `shatter-rust` executable. Doctor showed all green in that state (`audits/2026-09-22/cli-ux-transcripts/doctor.out`, exit 0).
>
> Added acceptance checks:
> - The Rust section also reports the runtime-crate location: the `SHATTER_RUNTIME_PATH` value (and whether `<value>/Cargo.toml` exists), else the auto-discovered path from the executable-ancestor walk, else "not found" with a one-line fix naming `SHATTER_RUNTIME_PATH`. The check reuses the same lookup logic as the frontend (move it into a shared helper or query it over the handshake) so doctor cannot disagree with execution.
> - Severity follows this issue's existing rule: a missing runtime crate is a warning, and a failure only when Rust is required. The require-flag spelling must match whatever str-qwua7.13 settles on (it proposes `--require-frontend <lang>`, this issue proposes `--require-rust`); pick one before implementing.
> - Test: a relocated `shatter-rust` with `SHATTER_RUNTIME_PATH` unset makes doctor report "runtime crate: not found"; with the variable set to the real crate it reports the path. The first case shows no runtime line (all green) on current main.
>
> The runtime error dedup, the `rust` failure-impact row and the env-var docs are tracked in the new issue **rust-runtime-path-and-doctor** (<filed id>). Toolchain and sandbox/host-write readiness in doctor are tracked in **doctor-execution-readiness** (<filed id>).

## Filing note

The filer script must replace each `<filed id>` with the ids assigned to rust-runtime-path-and-doctor (05) and doctor-execution-readiness (11), both in this bucket.

---

<!-- file: 10-rust-hint-once-note.md -->

---
slug: rust-hint-once-note
kind: note-to-existing
title: "Note on str-qwua7.13: the missing-Rust-frontend hint is still printed twice in explore and is contributor-oriented"
priority: P1
type: note
labels: [rust-frontend, ux, install, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.13
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.13

Target: `str-qwua7.13` (open, P1, "scan and run must agree on a missing frontend; print the remediation once"). Verified open with `bd show` on 2026-09-23. Action: add the comment below. str-qwua7.13 owns printing the remediation once; this note adds explore to its evidence and adds the hint-content requirement so a second issue does not compete with it.

## Comment text

> Audit 2026-09-22 (cli-ux-10): the duplicate hint also affects `explore`, not only `scan`. Exploring a `.rs` target with no `shatter-rust` available printed the hint twice (1,345 bytes of stderr; `audits/2026-09-22/cli-ux-transcripts/rust-explore.err`). The text is `RUST_FRONTEND_INSTALL_HINT` at `shatter-cli/src/helpers.rs:418-424` (used by `check_frontend_availability` at `helpers.rs:497`, which this issue already cites). It is about 600 characters of source-checkout guidance ("the expected state after `cargo build --release --bin shatter` from the workspace root") shown to every user.
>
> Added acceptance checks:
> - The CLI integration tests this issue already requires also cover `explore` on a `.rs` target: exactly one hint occurrence on stderr.
> - The hint is at most two lines: what is missing, and one install command or a pointer to `shatter doctor` (str-qwua7.40) for details. Source-checkout build instructions move to README "Build from source".
>
> Related new issue: **rust-runtime-path-and-doctor** (<filed id>), which covers the next failure a user hits after installing `shatter-rust` (the runtime crate).

## Filing note

The filer script must replace `<filed id>` with the id assigned to rust-runtime-path-and-doctor (05, this bucket).

---

<!-- file: 11-doctor-execution-readiness.md -->

---
slug: doctor-execution-readiness
kind: new
title: "`shatter doctor` does not check node/go toolchains or sandbox/host-write readiness, so it reports all green on a machine where every execution command refuses to run"
priority: P2
type: feature
labels: [doctor, ux, install, sandbox, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [sandbox-backend-disables-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `shatter doctor` does not check node/go toolchains or sandbox/host-write readiness

## Problem

`shatter doctor` exited 0 in the audit on a machine where no execution command would run: no sandbox backend and no `SHATTER_ALLOW_HOST_WRITES`, so default-deny (str-gg9v) refuses every `explore`/`scan`/`run`. It also says nothing about whether the node and go toolchains that the TS and Go frontends need are present. Split out of rust-runtime-path-and-doctor during the cross-check; the Rust frontend and runtime-crate checks belong to str-qwua7.40 (see doctor-rust-runtime-note).

## Evidence

Re-verified against the audit worktree (code at HEAD 56c86168):

- `shatter-cli/src/commands/doctor.rs:40-63`: doctor prints version and hashes, the project configuration report, `check_embedded_frontend` and `check_generated_paths_ignored`, and returns `Ok(frontend_ok && gitignore_ok)`. There are no toolchain or host-write checks.
- `shatter-cli/src/host_writes.rs:55-79`: `sandbox_backend_configured()` and `execution_permitted()` decide the default-deny gate; doctor never calls them.
- Transcript `audits/2026-09-22/cli-ux-transcripts/doctor.out`: version and hashes, project configuration, "Embedded Go frontend: up to date.", "Generated-path gitignore: all configured output paths are ignored.", exit 0.
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F10.

## Severity contract (defined here so a TS-only install never fails on unused prerequisites)

A language is **in use** when the resolved project root contains source files of that language that a scan would select (reuse `list-targets` selection, respecting config excludes) or the project config names it. A language is **required** only when the user passes the require flag that str-qwua7.13 settles on (`--require-frontend <lang>` is its proposal).

| Check | Not in use | In use | Required |
|---|---|---|---|
| node (TS) / go (Go) toolchain missing or below the minimum version | info line, no warning | warn | fail |
| Host-write readiness: neither a valid backend nor `SHATTER_ALLOW_HOST_WRITES`/`--allow-host-writes` available | — | warn ("execution commands will refuse to run; set ...") | warn |
| `SHATTER_SANDBOX_BACKEND` set to a value outside `none`/`bwrap`/`docker` | fail | fail | fail |

Default-deny with no opt-in is a safe, intended state, so it is never a failure. Existing failing checks (embedded frontend staleness, un-ignored generated paths) keep their current severity.

## Acceptance criteria

- [ ] Doctor prints a toolchain line for node and go (found path and version, or "not found"), with the severity from the table and a one-line fix command.
- [ ] Doctor prints a host-write readiness line that states what execution will do per language, using the per-frontend rule from sandbox-backend-disables-guard (for example "Go: runs under docker; TS/Rust: refused unless --allow-host-writes"). It calls the same helpers as the execution path, so doctor and execution cannot disagree.
- [ ] Exit status: non-zero iff any check is `fail` per the table. The contract is written into SPEC §2.9 with a §8 changelog row.
- [ ] Tests (CLI integration, temp project dirs, `PATH` controlled so node/go presence is deterministic):
  - TS-only project, go absent from PATH, no host-write opt-in: exit 0; go line is info, host-write line is warn.
  - TS-only project, node absent from PATH: exit 0, node line is warn.
  - Same, with the require flag for `ts`: non-zero exit.
  - `SHATTER_SANDBOX_BACKEND=dcoker`: non-zero exit, message names the accepted values.
  - The TS-project-with-no-opt-in case shows no host-write line on current main (all green), so it fails on main and passes after the fix. Record red and green runs in the close reason.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Add `check_toolchains` and `check_host_write_readiness` next to the existing `check_*` functions in `doctor.rs`, each returning a pass/warn/fail/info status that the caller folds into the exit code. Reuse `list-targets` language detection and the `host_writes.rs` helpers.

## Out of scope

- The Rust frontend and runtime-crate section (str-qwua7.40, with doctor-rust-runtime-note).
- Choosing the require-flag spelling (str-qwua7.13).

## Related

str-qwua7.40, str-qwua7.13, str-gg9v. In this bucket: sandbox-backend-disables-guard (blocks this: the readiness line must report its per-frontend rule), rust-runtime-path-and-doctor, doctor-rust-runtime-note.

## Priority / Type

P2, feature.

---

<!-- file: 12-rust-main-default-exclusion.md -->

---
slug: rust-main-default-exclusion
kind: new
title: "Rust frontend offers `fn main` as an explore/scan target; Go already excludes its `main` entrypoint"
priority: P3
type: bug
labels: [rust-frontend, parity, discovery, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust frontend offers `fn main` as an explore/scan target; Go already excludes its `main` entrypoint

## Problem

Exploring `rust/04_errors.rs` explores `main` as a third target alongside `safe_divide` and `compute_stats`. A binary entrypoint takes no inputs, and running it executes the program's side effects rather than exercising a function under test. The Go frontend already excludes the literal `main` entrypoint for this reason (str-jeen.55); the Rust frontend has no equivalent. Split out of per-language-outcome-rendering during the cross-check.

## Evidence

Verified against the audit worktree (code at HEAD 56c86168):

- `audits/2026-09-22/cli-ux-transcripts/rust-explore3.err` lines 1-4: `[progress] starting 3/3: main` for `04_errors.rs`, a file with `safe_divide`, `compute_stats` and `main`.
- Go model: `shatter-go/protocol/analyzer.go:465-490` `isMainEntrypointDecl` matches only the literal free function `main` with no params and no results in `package main`, so helper functions in the same package stay discoverable.
- TS model for name-based exclusion: `shatter-core/src/discovery.rs:605-617` (`LIFECYCLE_EXPORT_NAMES`, str-qwua7.56).
- `grep -n '"main"' shatter-rust/src/analyzer.rs` finds only the `#[tokio::main]` attribute check at line 357, no entrypoint exclusion.
- This draft's claim was not independently re-verified by the audit verifier (cli-ux-11 note). The first acceptance item makes the implementer confirm it.

## Acceptance criteria

- [ ] Reproduce first: a test that analyzes a `.rs` fixture containing `fn main()` plus two ordinary functions lists `main` as a default target on current main. If it does not, close this issue with that test output as the reason.
- [ ] The Rust analyzer excludes a free `fn main()` with no parameters (including `#[tokio::main] async fn main()`) at the crate root of a binary target from default explore/scan target lists, mirroring Go's `isMainEntrypointDecl`. A function named `main` inside a module, an `impl` block, or with parameters stays discoverable.
- [ ] Naming it explicitly (`shatter explore file.rs:main`) still explores it, or fails with a clear message that says it is excluded as an entrypoint; pick one and document it in `shatter-rust/CLAUDE.md`.
- [ ] Tests: the fixture above (red on main, green after), plus negative cases for a `mod x { pub fn main() }` and `impl T { fn main(&self) }`.
- [ ] The change alters `analyze` output, so update `protocol/parity-matrix.yaml` (entrypoint exclusion per frontend) and `shatter-rust/CLAUDE.md`, then run `task parity` and `task conformance`.
- [ ] `cargo test --test e2e_concolic_rust` passes. `task affected` passes, with `Gates selected` recorded.

## Out of scope

- Outcome rendering (per-language-outcome-rendering).

## Related

str-jeen.55 (Go main exclusion), str-qwua7.56 (TS lifecycle exclusion). In this bucket: per-language-outcome-rendering.

## Priority / Type

P3, bug (noise in reports; no wrong results beyond an extra target).

---

<!-- file: 13-rust-mocks-to-string-diagnosis.md -->

---
slug: rust-mocks-to-string-diagnosis
kind: new
title: "Diagnose why the Rust explore report lists `Mocks: to_string` for safe_divide"
priority: P3
type: task
labels: [rust-frontend, mocks, report, diagnosis, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Diagnose why the Rust explore report lists `Mocks: to_string` for safe_divide

## Problem

The explore report for `rust/04_errors.rs:safe_divide` lists `Mocks: to_string`. `safe_divide` presumably calls `"division by zero".to_string()`, a standard-library conversion that should not be mocked. Either the mock-recording source reports a symbol that was not replaced (a reporting bug), or `to_string` really is being replaced (an execution-correctness bug that could change observed outcomes). The audit did not determine which. Split out of per-language-outcome-rendering during the cross-check; the placement of the `Mocks:` line stays there.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/rust-explore3.out` line 10: `- *Mocks: to_string*` after the `safe_divide` table.
- `shatter-cli/src/render.rs:139-142` renders `opts.mocks_used`, which the explore command passes as `mock_symbols` (`shatter-cli/src/commands/explore.rs:3620-3628`).
- Not re-verified by the audit verifier (cli-ux-11 note).

## Acceptance criteria

This is a diagnosis issue. It closes with a written finding, not necessarily a fix.

- [ ] Trace where `mock_symbols` for a Rust target comes from (frontend response field and the shatter-rust code that fills it) and state it in the close reason with file:line references.
- [ ] Determine, with a test or a recorded run, whether `to_string` is actually replaced during execution of `safe_divide` (for example: does the `Err` payload still equal `"division by zero"` under exploration, and does the instrumented source substitute the call).
- [ ] Close reason states one of: (a) reporting-only bug, (b) real mock of a std method, (c) intended behavior, with the evidence. For (a) or (b), file a follow-up bug with a failing test attached (or fix it here if it is under ~20 lines, with the test red on main and green after). For (c), document the behavior in `shatter-rust/CLAUDE.md`.

## Related

In this bucket: per-language-outcome-rendering (renders the Mocks line).

## Priority / Type

P3, task (diagnosis).

---

<!-- file: 14-run-validity-degraded-cause-diagnosis.md -->

---
slug: run-validity-degraded-cause-diagnosis
kind: new
title: "Diagnose why `shatter run` rates an all-TS, fully supported example directory as 'degraded' (61.5% represented source)"
priority: P2
type: task
labels: [report, run, validity, diagnosis, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Diagnose why `shatter run` rates an all-TS, fully supported example directory as 'degraded'

## Problem

`shatter run .` over a directory of six TS example files, all in a supported language, reported `represented_source_percent=61.5 below high threshold 75.0` and a `degraded` verdict. Either the representation math is wrong (a bug that makes every run look worse than it is) or about 38% of the source lines really are unrepresented (for example module-level code, type declarations, or functions that failed or timed out), in which case the report must say so. Split out of run-report-verdict-and-coverage-metrics during the cross-check, because it is an open-ended investigation that should not hold up the layout fix.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/run.out`: `## Report Validity: degraded`, detail `represented_source_percent=61.5 below high threshold 75.0`.
- The classification is in `shatter-cli/src/commands/run.rs` `classify_validity` (degraded branch at run.rs:1706-1715), fed by `representation_spans` and the run summary built just before `run.rs:673`.
- Source findings: audit 2026-09-22 cli-ux-13. Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13.

## Acceptance criteria

This is a diagnosis issue. It closes with a written finding and, if the math is wrong, a fix with a test.

- [ ] Reproduce on the same six example files (record the examples commit and the command) and capture the JSON `unrepresented_*_lines` buckets.
- [ ] Attribute every unrepresented line to a bucket and a cause (per file, with line ranges), and state it in the close reason.
- [ ] If any attribution is wrong (lines counted as unrepresented that belong to explored functions, or non-executable lines such as imports and type declarations counted in the denominator), fix it here with a unit test on `classify_validity`/representation spans that fails on current main and passes after, and record both runs. If the fix is larger than a focused change, file it as a separate bug with the failing test attached, blocked by nothing, and close this one pointing at it.
- [ ] If the attribution is correct, record that and file (or comment on run-report-verdict-and-coverage-metrics with) the per-bucket wording the plain-language verdict must use for this case.
- [ ] `task affected` passes, with `Gates selected` recorded, if any code changed.

## Related

str-jeen.5 (introduced the validity layer). In this bucket: run-report-verdict-and-coverage-metrics.

## Priority / Type

P2, task (diagnosis). Priority matches the parent finding: a wrong representation figure would mislabel every run.

---

<!-- file: 15-coverage-headline-metric-unification.md -->

---
slug: coverage-headline-metric-unification
kind: new
title: "explore, scan and run headline three different coverage metrics without naming them; make line coverage the named headline everywhere"
priority: P2
type: feature
labels: [report, ux, coverage, explore, scan, run, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore, scan and run headline three different coverage metrics without naming them

## Problem

For the same TS example files:

- `explore` headlines **line** coverage: "100% coverage (7/7 lines)".
- `scan` headlines **branch** coverage: "Overall coverage (completed-functions subset): 92.5%" (37/40 branches), without saying it is branches.
- `run` totals **lines**: "80/99 81%".

`run` also uses a different default iteration budget from `scan`: for `computeStats`, run reports 31% and scan 63%. A user comparing commands cannot tell whether the numbers disagree or measure different things. Split out of run-report-verdict-and-coverage-metrics during the cross-check.

## Evidence

- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `ts-explore.out` line 21 (`100% coverage (7/7 lines)`), `ts-scan.out` line 28 (`Overall coverage (completed-functions subset): 92.5%`), `run.out` total row `| **Total** | **40** | **80/99** | **81%** |` and `computeStats` at `5/16 | 31%`.
- The cross-command comparison was not re-run by the audit verifier (cli-ux-13 note); the first acceptance item re-establishes it.
- Source findings: audit 2026-09-22 cli-ux-13. Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13.

## Contract (decided in this draft; the maintainer can override before filing)

One contract, not two:

1. **Headline metric = line coverage, for all three commands**, printed through one shared type (`CoverageHeadline { metric, covered, total }`) that always prints the metric name: `Line coverage: 80/99 (81%)`.
2. Branch coverage may appear as a **secondary**, named line (`Branch coverage: 37/40 (92.5%)`) below the headline, never as the headline.
3. Each headline states the iteration budget it was produced with when it differs from explore's default.

Rationale: two of the three commands already headline lines, and what the branch metric counts is under question in branch-metric-counts-sites (bucket shatter-reports-and-specs). Choosing lines means this issue does not wait on that one.

## Acceptance criteria

- [ ] Re-run the three commands on the same example files (record commit and commands) and record the three headline strings in the issue before changing code.
- [ ] `explore`, `scan` and `run` markdown reports print their headline through the shared type, as `Line coverage: <covered>/<total> (<pct>%)`, and any branch figure as a separately named secondary line. SPEC (reporting section) records the contract, with a §8 changelog row.
- [ ] **Metric-identity test** (fails on current main): on one deterministic single-file, single-function TS fixture with a fixed seed, the headline **denominators** (total lines) reported by explore, scan and run are equal, and each headline string starts with `Line coverage:`. On current main scan's headline is a branch figure (denominator 40 vs 99 lines), so this fails. A test that only checks the presence of a metric name does not satisfy this criterion.
- [ ] Budget: either `run` and `scan` share the default iteration budget, or the report prints the budget next to the headline; a test asserts whichever is chosen.
- [ ] JSON output keeps both line and branch figures under distinct, named fields (no field renamed without a changelog row).
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Out of scope

- What the branch metric counts (branch-metric-counts-sites).
- Validity verdict placement (run-report-verdict-and-coverage-metrics).

## Related

str-qwua7.57 (terminology unification, not metrics), str-4ad5, branch-metric-counts-sites. In this bucket: run-report-verdict-and-coverage-metrics, markdown-drops-render-plain-info (branch line in explore).

## Priority / Type

P2, feature.
