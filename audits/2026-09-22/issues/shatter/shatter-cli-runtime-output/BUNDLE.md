# Bundle: shatter-cli-runtime-output

- **Bucket:** shatter-cli-runtime-output
- **Repo:** shatter (bd in /home/ketan/project/shatter, prefix str)
- **Parent epic:** Epic: Audit 2026-09-22 findings
- **Theme:** What users see while and after running: sandbox guard bypass, scan progress, runtime-path hints, per-language errors, run report verdict and metrics.
- **Status:** drafts only. Nothing is filed (D6).
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
| 05 | rust-runtime-path-and-doctor | new | - | P2 | Following the Rust-frontend hint still fails on the undocumented SHATTER_RUNTIME_PATH, and `shatter doctor` reports all green |
| 06 | per-language-outcome-rendering | new | - | P2 | Error outcomes render inconsistently across languages; Rust shows truncated serde JSON |
| 07 | run-report-verdict-and-coverage-metrics | new | - | P2 | Run report opens with an unexplained 'degraded' verdict before its H1; explore, scan and run headline three different coverage metrics |
| 08 | markdown-drops-render-plain-info | new | - | P3 | Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason) |

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
- Docs that recommend the variable with no Go-only caveat:
  - `README.md:309-310`: "Recommended: run targets inside an OS sandbox (Go frontend)". The Go-only fact sits in a comment while the line is labelled Recommended.
  - `README.md:321-322` and `README.md:326-329` tell CI and wrapper users to prefer `SHATTER_SANDBOX_BACKEND`.
  - `QUICKSTART.md:83-85` recommends it for the TS example.
  - `SPEC.md:606-609` and `SPEC.md:622-624` say a configured backend "satisfies both controls at once".
  - `refusal_message()` at `shatter-cli/src/host_writes.rs:98-115` says "Configure an OS sandbox (recommended)".
  - The `--allow-host-writes` help at `shatter-cli/src/args.rs:170-171` calls the backend "the safer alternative".
- Repro from the audit, on main 9516036d. A TS target `touch()` calls `fs.writeFileSync('marker-' + s + '.txt', ...)`:
  - `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch --max-iterations 5` exits 0 and leaves `marker-long.txt` and `marker-short.txt` in the cwd.
  - The same run with `--allow-host-writes` instead leaves the cwd clean.
- Source findings: audit 2026-09-22 docs-01 (verdict confirmed, P1). Area evidence: `audits/2026-09-22/areas/docs.md`.

## Acceptance criteria

- [ ] For TS and Rust targets, a set `SHATTER_SANDBOX_BACKEND` never skips the `IsolationGuard`. Those frontends still get `SHATTER_HOST_WRITE_DIR`. The run prints one warning line: `SHATTER_SANDBOX_BACKEND applies to Go targets only; TS/Rust targets run in a throwaway directory`.
- [ ] Mixed-language runs (`scan`/`run` over a directory with TS, Go and Rust files) keep the guard for the non-Go frontends. Go keeps using its backend. Go already ignores `SHATTER_HOST_WRITE_DIR` when `Runner.Enabled()` is true (`shatter-go/CLAUDE.md` host-write paragraph).
- [ ] A backend value other than `none`, `bwrap` or `docker` is rejected up front with a clear error. It is never treated as a sandbox.
- [ ] Decide explicitly whether a TS- or Rust-only execution with only `SHATTER_SANDBOX_BACKEND` set passes the default-deny gate, and record the decision in SPEC §2.10. Recommended: it passes, is equivalent to `--allow-host-writes`, and prints the warning above.
- [ ] Regression tests, one per frontend (TS, Go, Rust), each run as a CLI integration test from a temp cwd. A target that writes a relative file, run under each opt-in (`--allow-host-writes`, `SHATTER_ALLOW_HOST_WRITES=1`, and `SHATTER_SANDBOX_BACKEND=docker`), leaves the cwd unchanged. For Go, where docker or bwrap is not available on the runner, the test may use a stub backend or skip with a logged reason. The TS and Rust `SHATTER_SANDBOX_BACKEND` cases must **fail on current main and pass after the fix**. Record both runs in the close reason.
- [ ] README ("Executing Target Functions Safely" and the CI paragraph), QUICKSTART, SPEC §2.10, the `refusal_message()` text and the `--allow-host-writes` help state that OS sandbox backends are Go-only and recommend `--allow-host-writes` or `SHATTER_ALLOW_HOST_WRITES=1` for TS and Rust.
- [ ] SPEC §8 has a changelog row for the behavior change.
- [ ] `protocol/parity-matrix.yaml` records sandbox-backend support per frontend (Go yes, TS no, Rust no), and `task parity` passes.
- [ ] `task affected` passes, with its `Gates selected` output recorded in the close reason.

## Suggested approach

Make the "is this contained?" decision per frontend, not per process:

1. Always create the `IsolationGuard` for execution commands, whether or not a backend is set, and export `SHATTER_HOST_WRITE_DIR`. Go already ignores it when its sandbox is enabled, so the TS and Rust redirect comes back without any Go change. This is the smallest correct change and removes the early `return Ok(None)` at `host_writes.rs:142-145`.
2. Validate the backend value in `sandbox_backend_configured()` against the set Go accepts.
3. Drive the per-frontend rule from data: add a `sandbox_backends` capability row to `protocol/parity-matrix.yaml` so a future TS or Rust backend is a matrix change plus implementation, not a hidden CLI assumption.

## Out of scope

- Implementing OS sandbox backends for TS or Rust.
- Redirecting relative writes under `--allow-host-writes` for TS paths not yet covered. That is str-joyqu (open).
- The SPEC changelog backfill errors from str-qwua7.8. That is spec-changelog-backfill.

## Related

str-gg9v (original default-deny), str-02i70 (per-frontend throwaway-dir redirect), str-joyqu, str-qwua7.8 (closed; its addendum criterion required the docs never to treat the Go-only variable as proof of confinement), sa-oio (shatter-agents note that the backend is Go-only). The comment on str-qwua7.8 is docs-first-run-reopen-note.

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
> 1. **Sandbox remedy (docs-01, P1).** The addendum required that the TS example "never treat the Go-only backend variable as proof of confinement". README.md:309-310 and 326-329, QUICKSTART.md:83-85, SPEC.md:606-609 and 622-624, the `refusal_message()` text (shatter-cli/src/host_writes.rs:98-115) and the `--allow-host-writes` help (shatter-cli/src/args.rs:170-171) all still recommend `SHATTER_SANDBOX_BACKEND` with no Go-only caveat. It is worse than a docs gap: `host_writes.rs:142-145` skips the throwaway-directory IsolationGuard whenever the variable is set, so a TS or Rust target that writes a relative path writes straight into the cwd. This was reproduced with `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch`, which left `marker-*.txt` in the cwd. Tracked in the new issue **sandbox-backend-disables-guard** (<filed id>).
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

> Audit 2026-09-22: this behavior has regressed. Default `shatter scan` no longer shows live progress. `shatter-cli/src/commands/scan.rs:1412-1424` computes `scan_start.elapsed()` once after the scan returns, then loops over `result.function_results` and logs `[i/N] <fn> (<elapsed>s elapsed)`. All N lines appear together after the scan, with the same elapsed value (for example ten lines of `(26.0s elapsed)`). Nothing is printed while the scan runs. `scan --progress` is live but interleaves raw JSON objects with human `[info]` lines on stderr. `shatter run` prints no progress at all (0 bytes of stderr over 74 s). Evidence: `audits/2026-09-22/cli-ux-transcripts/ts-scan.err`, `scan-progress.err` and `run.err`, and finding cli-ux-06. Tracked in the new issue **scan-progress-post-hoc** (<filed id>), which also adds a test asserting that progress lines reach stderr before the final report.

## Filing note

The filer script must replace `<filed id>` with the id assigned to scan-progress-post-hoc (this bucket, 03).

---

<!-- file: 05-rust-runtime-path-and-doctor.md -->

---
slug: rust-runtime-path-and-doctor
kind: new
title: "Following the Rust-frontend hint still fails on the undocumented SHATTER_RUNTIME_PATH, and `shatter doctor` reports all green"
priority: P2
type: bug
labels: [rust-frontend, docs, ux, install, doctor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Following the Rust-frontend hint still fails on the undocumented SHATTER_RUNTIME_PATH, and `shatter doctor` reports all green

## Problem

A user who explores a `.rs` target outside the shatter source tree hits a chain of failures, and nothing in the tooling explains it:

1. **Without `shatter-rust`**, explore prints a roughly 600-character, contributor-oriented hint twice (1,345 bytes of stderr). It frames the situation as "the expected state after `cargo build --release --bin shatter` from the workspace root", which is source-checkout guidance shown to every user.
2. **With `shatter-rust` on PATH**, as the hint instructs, every function fails with `execute error (FileNotFound): cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`. The error repeats once per function, and the "Failure impact" table labels the language `any` although the target is `.rs`.
3. **`SHATTER_RUNTIME_PATH` is undocumented.** It does not appear in README, QUICKSTART, SPEC or any `--help` text.
4. **`shatter doctor` exits 0** and checks none of this: not the Rust frontend, the runtime crate, the node or go toolchains, or sandbox and host-write readiness. Without sandbox or host-write readiness, every execution command refuses to run.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- The hint text is `RUST_FRONTEND_INSTALL_HINT` at `shatter-cli/src/helpers.rs:418-424` (the audit cited line 497, which is stale).
- The runtime lookup and the error are at `shatter-rust/src/executor.rs:1200-1223`. `SHATTER_RUNTIME_PATH` is read first, and the fallback error is `"cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH"`.
- `shatter-cli/src/commands/build_frontend.rs:607-630` is the only other place that mentions the variable (warnings during `build-frontend`).
- `shatter-cli/src/commands/doctor.rs:59-61` runs `check_embedded_frontend` and `check_generated_paths_ignored` plus the project-configuration report. There are no toolchain, runtime-crate or sandbox checks.
- `grep -n SHATTER_RUNTIME_PATH README.md QUICKSTART.md SPEC.md` returns no matches.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `rust-explore.err` (1.3 KB) has the hint printed twice.
  - `rust-explore2.err` has the runtime-crate error three times, plus the `any` language label.
  - `doctor.out` shows version and hashes, the project configuration, "Embedded Go frontend: up to date." and "Generated-path gitignore: all configured output paths are ignored." It exits 0.
- Root cause of the missing test coverage: the E2E Rust suite (`shatter-core/tests/e2e_concolic_rust.rs`) runs from the workspace root, where the runtime crate is found automatically, so the out-of-tree path is never exercised.
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F10.

## Acceptance criteria

- [ ] `shatter doctor` reports each of the following with a pass/warn/fail status and a one-line fix command:
  - Rust frontend resolution: which `shatter-rust` is found, or why none is.
  - shatter-rust-runtime crate location: `SHATTER_RUNTIME_PATH` value, or the auto-discovered path, or "not found".
  - node and go toolchain presence and version.
  - Sandbox and host-write readiness: whether a backend or `SHATTER_ALLOW_HOST_WRITES` is set, and what execution commands will do.
- [ ] `doctor` exits non-zero when a check that blocks execution fails. It stays exit 0 for warnings, such as "Rust frontend absent; Rust targets will be skipped". The exit codes are documented in SPEC §2.9.
- [ ] The missing-frontend hint is at most two lines, printed once per run, and points at `shatter doctor` for details. Source-checkout build instructions move to README "Build from source".
- [ ] The runtime-crate failure is reported once per run, not once per function, and names `SHATTER_RUNTIME_PATH` with an example value. The failure-impact table shows `rust` for `.rs` targets.
- [ ] `SHATTER_RUNTIME_PATH` is documented in the README install section and in the env-var table that str-qwua7.20.2 adds, until str-qwua7.60 embeds the runtime.
- [ ] A test runs a Rust explore from a temp directory outside the repo with `SHATTER_RUNTIME_PATH` unset, and asserts either success or the single actionable error. A second test asserts `doctor`'s runtime-crate check output in the same setup. Both must fail on current main (repeated error or all-green doctor) and pass after the fix. Record both in the close reason.
- [ ] SPEC §2.9 and §8 are updated. `task affected` passes, with `Gates selected` recorded, and `cargo test --test e2e_concolic_rust` passes.

## Suggested approach

Add doctor checks as small, independent functions that follow the existing `check_*` pattern in `doctor.rs`. Reuse the frontend-resolution helper in `helpers.rs` so doctor and explore agree on what "found" means. Reuse `sandbox_backend_configured()` and `execution_permitted()` from `host_writes.rs` for the readiness line. Once sandbox-backend-disables-guard lands, report the backend as Go-only.

## Out of scope

- Embedding the Rust frontend or runtime in the shatter binary (str-qwua7.60).
- The general SHATTER_* env-var table (str-qwua7.20.2). This issue only adds the `SHATTER_RUNTIME_PATH` row, if that table exists by then.

## Related

str-qwua7.40 (open; extends doctor to Rust-frontend resolution. Coordinate with it or fold it in), str-qwua7.20.2, str-qwua7.60, str-qwua7.13 (prints the hint once), sandbox-backend-disables-guard (the readiness line should reflect its Go-only rule).

## Priority / Type

P2, bug.

---

<!-- file: 06-per-language-outcome-rendering.md -->

---
slug: per-language-outcome-rendering
kind: new
title: "Error outcomes render inconsistently across languages; Rust shows truncated serde JSON"
priority: P2
type: bug
labels: [report, ux, parity, rust-frontend, go-frontend, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Error outcomes render inconsistently across languages; Rust shows truncated serde JSON

## Problem

The same logical outcome, "division by zero", from the three `04-errors` examples renders three different ways in the explore report:

- **TS:** ``throws `Error: division by zero` ``. This is correct.
- **Go:** ``throws `function_error: division by zero` ``. Go functions return errors rather than throw, and `function_error` is an internal outcome category name.
- **Rust:** ``returns `{"Err":"division by zero"}` ``. Successful struct results render as ``returns `{"Ok":{"avg":2.0,"flag":null,"max":2....` ``, which is a raw serde JSON envelope cut off mid-token.
- Rust sections also end with a stray list item, `- *Mocks: to_string*`, placed directly after the table with no heading. `main` is explored as a target.

The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`) requires "the error value in languages where errors are scalars (e.g., Go's `error` string, Rust's enum variant)".

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/render.rs:277-284` `value_short()` does `let s = v.to_string(); if s.len() > 40 { format!("{}...", &s[..37]) }`. It truncates the JSON by **byte** count. That causes the mid-token cut. From reading the code (not run), `&s[..37]` also panics when byte 37 is not a UTF-8 character boundary, for example when a string outcome contains non-ASCII text.
- The "Mocks:" line comes from `shatter-cli/src/render.rs:141` (`extras.push(format!("Mocks: {}", ...))`), which is rendered as a trailing list item.
- The markdown report formatter in core is `format_exploration_report` at `shatter-core/src/explorer.rs:2950`.
- TS lifecycle-export exclusion lives at `shatter-core/src/discovery.rs:605-617` (`LIFECYCLE_EXPORT_NAMES` and `is_lifecycle_export_name`, from str-qwua7.56). There is no Rust `main` equivalent.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `ts-safedivide--concolic.out`: the TS form.
  - `go-explore.out`: ``throws `function_error: division by zero` ``.
  - `rust-explore3.out` lines 9-19: `{"Err":...}`, `{"Ok":{"avg":2.0,"flag":null,"max":2....`, `- *Mocks: to_string*`, and a `main` section.
- The examples are `ts/04-errors.ts`, `go/04-errors.go` and `rust/04_errors.rs` in the examples checkout (`SHATTER_EXAMPLES_DIR`).
- Source findings: audit 2026-09-22 cli-ux-11 (confirmed, P2; the Mocks line and `main` were not re-verified by the verifier). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F11.

## Acceptance criteria

- [ ] One per-language outcome formatter in `shatter-core`, used by both the core markdown formatter and `shatter-cli/src/render.rs`:
  - TS exceptions: `throws Error: <msg>` (unchanged).
  - Go error returns: `errors: <msg>`. The internal `function_error` never appears in rendered output.
  - Rust `Err(e)`: `returns Err(<msg>)`. Rust `Ok(v)`: `returns Ok(<v pretty>)`.
- [ ] Elision happens at a token or character boundary and never splits a UTF-8 character. A unit or property test (proptest over arbitrary strings, including multibyte ones) shows the truncation helper never panics and always yields valid UTF-8 with a closing ellipsis marker. This test must fail (panic) on current `value_short` for a non-ASCII case.
- [ ] A cross-language golden test renders the three `04-errors` examples and asserts equivalent outcome wording for the division-by-zero case and for one `Ok` or success case.
- [ ] Rust `fn main` is excluded from default explore/scan targets, the way TS lifecycle exports are, and can still be explored when named explicitly.
- [ ] The `Mocks:` line renders only when mocks are in effect, as a labelled line under the section heading. It no longer renders as a stray bullet after the table. Confirm whether `to_string` really is being mocked for `safe_divide`. If it is not, fix the mock-recording source.
- [ ] If rendered output is protocol-visible per `protocol/parity-matrix.yaml`, update the matrix and the affected frontend CLAUDE.md and run `task parity`. Otherwise record in the close reason that no parity change is needed.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Put an `OutcomeDisplay` function in core that takes `(language, outcome category, value)` and returns a short display string. Rust `Result` envelopes are recognised by their single `Ok` or `Err` key. Replace both truncation sites with one `elide(s, max_chars)` helper that cuts on `char_indices` and prefers the last JSON token boundary before the limit.

## Out of scope

- Changing protocol outcome categories or the wire format.
- The per-path constraint column (see markdown-drops-render-plain-info).

## Related

str-qwua7.56 (closed; TS lifecycle-export exclusion, the model for excluding Rust `main`).

## Priority / Type

P2, bug.

---

<!-- file: 07-run-report-verdict-and-coverage-metrics.md -->

---
slug: run-report-verdict-and-coverage-metrics
kind: new
title: "Run report opens with an unexplained 'degraded' verdict before its H1; explore, scan and run headline three different coverage metrics"
priority: P2
type: bug
labels: [report, ux, run, scan, explore, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Run report opens with an unexplained 'degraded' verdict before its H1; explore, scan and run headline three different coverage metrics

## Problem

**1. The verdict comes before the title and is not explained.** `shatter run .` over a directory of six TS files, all in supported languages, begins its stdout with `## Report Validity: degraded` and a one-row table whose detail reads `represented_source_percent=61.5 below high threshold 75.0`. Only after that does `# Shatter Run Report` appear. The reader sees a verdict, then the title. The verdict uses an internal field name and threshold, and does not say which source is unrepresented or why, in a directory where every file is supported. The recommended action, "Inspect unrepresented_*_lines buckets", names JSON fields the markdown report does not show.

**2. Three commands, three coverage metrics, none of them named consistently.** For the same example files:

- `explore` headlines **line** coverage: "100% coverage (7/7 lines)".
- `scan` headlines **branch** coverage: "Overall coverage (completed-functions subset): 92.5%" (37/40 branches).
- `run` totals **lines**: "80/99 81%".

`run` also uses a different default iteration budget from `scan`. For the same function, `computeStats`, run reports 31% and scan reports 63%. A user comparing commands cannot tell whether the numbers disagree or measure different things.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/run.rs:670-674` renders `render_validity_markdown(...)` and prints it with `print_markdown` *before* `print_summary_report(...)` at `run.rs:679`, which writes the H1.
- `render_validity_markdown` at `shatter-cli/src/commands/run.rs:1989-2010` writes `## Report Validity: {label}` and then a raw Reason/Detail/Recommended-action table.
- The degraded reason text is at `shatter-cli/src/commands/run.rs:1706-1715`: `represented_source_percent={rep_pct:.1} below high threshold {HIGH_REPRESENTATION_PCT:.1}` with "Inspect unrepresented_*_lines buckets ...".
- The H1 `# Shatter Run Report` is written at `shatter-cli/src/commands/scan.rs:2034` (the shared summary writer).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `run.out` has a blank line 1, `## Report Validity: degraded` on line 2, the reason row on line 6, and `# Shatter Run Report` on line 8. The total row is `| **Total** | **40** | **80/99** | **81%** |`, with `computeStats` at `5/16 | 31%`.
  - `ts-scan.out` line 28 reads `Overall coverage (completed-functions subset): 92.5%`.
  - `ts-explore.out` line 21 reads `100% coverage (7/7 lines)`.
- Source findings: audit 2026-09-22 cli-ux-13 (confirmed, P2; the cross-command metric comparison was not re-run by the verifier). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13. This was also 2026-09-04 usability-ui item 16, which was never filed.

## Acceptance criteria

- [ ] The `run` report's first non-blank line is its H1. The validity verdict follows directly under it as one plain-language sentence that states the cause and the affected files or functions. Example: "Validity: degraded. 38% of source lines are in functions that were not explored (3 failed, 2 unsupported); see Unrepresented source below." Internal field names such as `represented_source_percent` appear only in JSON output.
- [ ] The degraded cause in the audit's all-TS repro is identified and either fixed (if the representation math is wrong) or explained (if the lines really are unrepresented, for example as module-level code). Record which one in the close reason.
- [ ] `explore`, `scan` and `run` headline the same named coverage metric, or each headline states which metric it is ("line coverage", "branch coverage"). Pick one metric as the default headline for all three commands and record the choice in SPEC (reporting section) with a §8 changelog row.
- [ ] If `run` and `scan` keep different default iteration budgets, the report states the budget next to the coverage figure. Otherwise align the defaults.
- [ ] A golden or snapshot test covers the top of the `run` markdown report (H1 first, then the verdict sentence) and the headline coverage line of each of explore, scan and run on one shared example, asserting that the metric is named. The H1-first assertion must fail on current main. Existing tests at `run.rs:3490-3513` are updated.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Move the validity block into the summary writer, after the H1. Give `ValidityReason` a `human_summary` alongside the machine `detail`. Add a small shared `CoverageHeadline { metric, covered, total }` type in core that all three commands render through, so the metric name is always printed and cannot drift. Decide the default metric together with branch-metric-counts-sites, which questions what the branch metric counts.

## Out of scope

- The scan report's double H1 (`# Scan Results` then `# Shatter Scan Report`), absolute paths in function columns, and "Interesting Inputs" curation. Those are scan-report-headline-and-paths (bucket shatter-reports-and-specs).
- Changing the validity thresholds.
- Progress output (scan-progress-post-hoc).

## Related

str-jeen.5 (closed; introduced the `report_validity` layer), str-qwua7.57 (terminology unification, not metrics), str-4ad5, branch-metric-counts-sites, scan-report-headline-and-paths.

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

The default markdown report shows only the path count and line coverage. Neither mode says why exploration stopped (worklist exhausted, iteration budget or timeout), although `stop_reason` is in the artifact JSON. The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`, items 2, 4 and 7) treats branch coverage, discovery method and termination reason as essential for a human reader. `--render plain` therefore cannot be removed without losing information.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/args.rs:41` documents `plain` as "Legacy plain ANSI text output (deprecated)".
- The plain renderer's lines come from `shatter-core/src/coverage_metrics.rs:269-277` (`Branches: {covered}/{total} ...`) and `:328` (`Symbolic: {}/{} constraints ...`).
- The markdown path is `format_exploration_report` (`shatter-core/src/explorer.rs:2950`) and `shatter-cli/src/render.rs`. Neither emits the branch or discovery lines. `render.rs` never references `stop_reason`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `render-plain.out` has the `Branches:`, `[random: ...]` and `Symbolic:` lines, and `render-md-color.out` has 0 `Branches` lines.
- Source findings: audit 2026-09-22 cli-ux-14 (confirmed; the verifier lowered it to P3 because this is report richness, not wrong output). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F14. The termination reason was also 2026-09-04 usability-ui item 12, which was never filed.

## Acceptance criteria

- [ ] For each explored function, the default markdown explore report includes:
  - branch coverage (`Branches x/y`, with the metric named)
  - the discovery-method breakdown (for example `found by: random 3, concolic 0`)
  - a `Stopped: <reason>` line mapped from `stop_reason` to plain words (worklist exhausted / iteration budget reached / timeout)
- [ ] Every fact the plain renderer shows is present in markdown, so `--render plain` can be removed without losing information. Removing it is optional here and can be part of the flag unification.
- [ ] A golden test on `ts/01-arithmetic.ts:classifyNumber` asserts the three lines above. It must fail on current main.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Reuse the `coverage_metrics.rs` computations and add markdown variants next to the ANSI ones rather than recomputing. Land this together with, or right after, explore-format-flag-ignored (the `--render`/`--format` unification, bucket shatter-cli-flags-and-help), so the surviving renderer is the complete one.

## Out of scope

- A per-path input-constraint column. It needs SymExpr pretty-printing; file it as a follow-up if wanted.
- Turning the entire walkthrough-review rubric into a gate.

## Related

str-zt4v, str-qwua7.15 (covered only where `--render` appears in help), explore-format-flag-ignored (cli-ux-02), str-qwua7.10 (walkthrough gate checks exit codes only).

## Priority / Type

P3, feature.
