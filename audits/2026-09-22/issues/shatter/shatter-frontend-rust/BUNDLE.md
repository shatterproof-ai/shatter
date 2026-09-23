# Audit 2026-09-22 — bucket `shatter-frontend-rust` (repo: shatter)

Theme: Rust frontend, runtime and shatter-llm: crate-bridge stdout, match-arm constraint encoding, build/request timeout budgets, panic boundary, unsigned-int inputs, duplicated tests, silent test skips, LLM crate hardening.

Tracker: bd in /home/ketan/project/shatter (prefix `str`). Parent epic for every item: "Epic: Audit 2026-09-22 findings". Nothing in this bundle has been filed. Evidence was re-verified against the audit worktree (main 16794cef plus audit files; no code change in the cited paths since audit commit 56c86168). Revised 2026-09-23 after the Codex cross-check (`crosscheck/shatter-frontend-rust.codex.md`); see `REVISION.md` in this directory.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire snapshot `shatter diff` and the unused Snapshot writer path. spec-diff is the regression tool, and SPEC/README/QUICKSTART are updated to match. The `diff` name becomes free; whether str-81xiw takes it is left to that epic. The shatter-agents docs for `shatter diff --staged` are corrected.
- **D3 Concolic positioning:** measure first. P1 benchmark comparing default and concolic, and P1 fix for concolic early termination. A later decision issue re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and move tracker sync to a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering the DB. AGENTS.md drops `bd sync`, str-qwua7.28 is superseded, and bento gets matching guidance. No hook-timeout env var or bypass guidance.
- **D5 Git identity:** the leaked `[user]` section has already been removed. Add `.mailmap`, a git-state check (fold into str-qwua7.1), and a before/after `.git/config` snapshot in the fixture-isolation test.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

Effect on this bucket: D1 applies to `rust-crate-bridge-stdout` (the fix must work on, or at least compile for, the retained Windows target). D2-D5 do not touch any item here. D6: nothing here is filed by an agent.

## Contents

| # | Slug | Kind | Target | Priority | Blocked by |
|---|---|---|---|---|---|
| 01 | rust-crate-bridge-stdout | new | - | P1 | - |
| 02 | rust-instrument-constraints | new | - | P2 | qwua7-36-escaping-repro (= str-qwua7.36) |
| 03 | timeout-budget-invariant | new | - | P2 | rust-build-deadline-enforcement |
| 04 | qwua7-50-panic-boundary | note-to-existing | str-qwua7.50 | P2 | - |
| 05 | rust-usize-negative-inputs | new | - | P2 | - |
| 06 | rust-usize-reopen-note | reopen-note | str-ddxe | P2 | rust-usize-negative-inputs (filing order only) |
| 07 | rust-main-duplicate-module-tree | new | - | P2 | - |
| 08 | rust-tests-offline-silent-pass | new | - | P2 | - |
| 09 | rust-frontend-design-dedupe | new | - | P3 | - |
| 10 | rust-runtime-harness-loop | new | - | P3 | - |
| 11 | shatter-llm-hardening | new | - | P3 | - |
| 12 | qwua7-21-llm-seed-oracle-docs | note-to-existing | str-qwua7.21 | P3 | - |
| 13 | qwua7-36-escaping-repro | note-to-existing | str-qwua7.36 | P2 | - |
| 14 | rust-build-deadline-enforcement | new | - | P2 | - |
| 15 | rust-input-deserialize-classification | new | - | P2 | - |
| 16 | rust-axum-extractor-classifier-dedupe | new | - | P3 | - |
| 17 | shatter-llm-parse-validation | new | - | P3 | rust-usize-negative-inputs |
| 18 | shatter-llm-backoff-cap | new | - | P3 | - |
| 19 | llm-model-rejection-diagnostic | new | - | P3 | - |


---

<!-- file: 01-rust-crate-bridge-stdout.md -->

---
slug: rust-crate-bridge-stdout
kind: new
title: "Rust crate-bridge harness shares stdout with user code: any target that prints fails as internal_error"
priority: P1
type: bug
labels: [rust-frontend, crate-bridge, harness, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust crate-bridge harness shares stdout with user code: any target that prints fails as internal_error

## Problem

The crate-bridge driver writes its JSON result line with `println!` to the same stdout the target function writes to. It redirects no file descriptors. The reader in `PersistentHarness::execute` treats the first stdout line as the protocol response. So when a target function calls `println!` (or `print!` without a newline), the protocol line is corrupted, and the execution is reported as `internal_error` / `runtime_failed` instead of returning the function's value.

Crate-bridge is not an opt-in mode. It is the automatic fallback whenever the bin-only dispatch path returns `NonExecutable` for a file inside a crate, so real crates hit it without asking. The standalone and dispatch harnesses already redirect fd 1/fd 2 with `libc::dup2`. Only crate-bridge does not.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files; line numbers unchanged since 56c86168):

- `shatter-rust/src/executor.rs:5195`: the generated crate-bridge driver emits `println!("{}", serde_json::to_string(&exec_result).unwrap());`, and nothing around it redirects fds.
- `dup2` appears only in the standalone generator (`executor.rs:2535-2561`) and the dispatch generator (`executor.rs:2714-2881`). Both emit unconditional Unix-only code (`libc::dup`, `std::os::unix::io::AsRawFd`) with no `cfg(windows)` branch, and crate-bridge cannot add a `libc` dependency to the user's crate.
- `executor.rs:627-647` `PersistentHarness::execute` reads one line from `response_rx` and runs `serde_json::from_str(&line)`. On failure it returns `OutputParseError("failed to parse execute result: ...\nline: {line}")`.
- `executor.rs:6292-6300`: crate-backed files go to bin-only and fall back to crate_bridge unless the caller requested a harness mode explicitly.
- `protocol/parity-matrix.yaml:492,496` and `shatter-rust/CLAUDE.md:21,23` record only that "crate-bridge does not capture console output". They do not record that user output corrupts the protocol channel.
- `.github/workflows/release.yml:87-93` ships `shatter-rust.exe` for `x86_64-pc-windows-msvc`, and maintainer decision D1 (2026-09-23) keeps Windows in the release matrix. A POSIX-only fix would leave crate-bridge on Windows either broken or uncompilable.
- Audit probe (finding frontend-rust-01): a crate containing `pub fn noisy(n: i64) -> i64 { println!("hello from user code {n}"); n + 1 }`, executed with `harness_mode: crate_bridge`, returned `error internal_error output parse error: failed to parse execute result: expected value at line 1 column 1 line: hello from user code 1`, with outcome `runtime_failed`. A sibling non-printing function worked after the harness restarted.

## Acceptance criteria

- [ ] Protocol responses travel on a channel that user code cannot write to through `print!`/`println!`/`eprint!`/`std::io::stdout()`. A sentinel or prefix scheme on the shared stdout pipe does **not** satisfy this item: a partial `print!` line concatenates with the response, and user output can imitate any sentinel.
- [ ] The design works on every platform the release matrix ships `shatter-rust` for, including `x86_64-pc-windows-msvc` (D1). The close note names the mechanism per platform (e.g. POSIX `dup`/`dup2` via an `extern "C"` shim on Unix and `SetStdHandle`/a separate handle on Windows, or a platform-neutral response file/pipe whose path the frontend passes to the driver). If the Windows path cannot be exercised in CI yet, the generated driver must at least compile for `x86_64-pc-windows-msvc` (`cargo check --target x86_64-pc-windows-msvc` on a generated crate-bridge crate, output pasted), and the close note links the issue tracking Windows harness execution.
- [ ] Regression tests, each shown failing on main and passing on the branch (paste both runs into the close note), over a crate-bridge-routed crate in one persistent harness process:
  - `println!` before returning: `noisy(1) == 2`, no `internal_error`.
  - `print!` with no trailing newline before returning.
  - user output that is itself valid JSON or looks like an execute result (e.g. `println!("{{\"return_value\":99}}")`), which must not be taken as the response.
  - writes to stderr.
  - three successive requests on the same harness process, alternating printing and non-printing functions, each returning its own correct value (no response shifted onto the next request).
- [ ] Console output from crate-bridge is either captured as `console_output` or deliberately discarded. Whichever is chosen, `protocol/parity-matrix.yaml` and the side-effect contract in `shatter-rust/CLAUDE.md` state it, and `task parity` plus `task conformance` pass.
- [ ] A new known-answer case in `shatter-core/tests/e2e_concolic_rust.rs` routes a printing target through crate-bridge (assert the harness mode in the test). Proof: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (the body of `task e2e-rust-governed`). Plain `cargo test --test e2e_concolic_rust` is not proof: every case is `#[ignore]`d and it reports success with 0 run. Paste the `test result:` line; it must show 0 ignored and include the new test.

## Suggested approach

Save the original stdout once at driver startup, point fd 1/fd 2 (or the Windows std handles) at a capture file or null device around each call, and write responses only to the saved handle. The standalone and dispatch generators do the per-call part already on Unix. Because crate-bridge cannot add `libc` to the user's crate, use a small `extern "C"` declaration for `dup`/`dup2` under `cfg(unix)` and the `SetStdHandle`/`GetStdHandle` Win32 calls under `cfg(windows)`, or avoid fd juggling entirely with a response file whose path comes in the request.

## Out of scope

- Capturing other side-effect kinds in crate-bridge.
- Changing the bin-only / crate-bridge routing policy.
- Malformed-request handling in the same generated loop (`executor.rs:5179-5182`), tracked by `rust-runtime-harness-loop`.
- Making the standalone and dispatch harnesses Windows-capable (they are Unix-only today; file separately if not already tracked).

## Size

M

## References

- Finding frontend-rust-01 (audit 2026-09-22, `audits/2026-09-22/findings.json`). Old draft: `drafts/shatter-code/58-rust-crate-bridge-stdout.md`.
- Related closed crate-bridge work (not duplicates): str-qsrb, str-70m2.
- Maintainer decision D1 (Windows and aarch64 releases kept), `audits/2026-09-22.md`.

---

<!-- file: 02-rust-instrument-constraints.md -->

---
slug: rust-instrument-constraints
kind: new
title: "Rust match arms are encoded as string equality on pattern text: ranges, bindings, guards and wildcards give Z3 nothing to solve"
priority: P2
type: bug
labels: [rust-frontend, instrumentation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [qwua7-36-escaping-repro]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust match arms are encoded as string equality on pattern text: ranges, bindings, guards and wildcards give Z3 nothing to solve

## Problem

Both the shatter-rust instrumentor (runtime constraints) and the analyzer (static branch conditions) encode each match arm as `eq(param "<scrutinee token text>", const str "<pattern token text>")`. For `match n { 0 => .., 1..=5 => .., k if k > 100 => .., _ => .. }` with `n: i64`, the runtime constraints are `n == "0"`, `n == "1 ..= 5"`, `n == "k"` and `n == "_"`. The analyzer's static builder encodes the binding arm `k if k > 100` as `n == "k"` (a string constant) and ignores the guard. Z3 cannot solve any of these as integer conditions, so concolic exploration cannot target match arms.

The core works around part of this for enums by matching raw token text alphanumerically (str-mambd). That does not help integer ranges, bindings, guards or wildcards.

This issue is only the match-arm semantics. Replacing the hand-formatted JSON with typed `protocol::SymExpr` + serde (and the escaping bugs that causes) is existing open **str-qwua7.36**; this issue builds on its shared builder and is blocked by it (the blocker slug `qwua7-36-escaping-repro` resolves to str-qwua7.36).

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-rust/src/instrument.rs:413-446` `visit_expr_match_mut`: builds `{"kind":"bin_op","op":"eq","left":{"kind":"param","name":"<match expr tokens>"},"right":{"kind":"const","type":"str","value":"<pattern tokens>"}}` for every arm, including bindings and wildcards. The param node has no `path` field.
- `shatter-rust/src/analyzer.rs:1750-1771` walks match arms without looking at `arm.guard`. `analyzer.rs:2128-2137` (`build_pattern_sym_expr`, `syn::Pat::Ident`) produces `scrutinee == Const(Str(ident))` for a binding.
- `shatter-rust/CLAUDE.md:81-83` describes the core-side token-text workaround (str-mambd).
- Audit probe (finding frontend-rust-03, not re-run): for `clean(n)` (`n + 1 > 5`, then the 4-arm int match above), all 60 concolic iterations used n=0 and reached 3 of 7 returns, while the batch line reported "6 paths, 5/7 branches".

## Acceptance criteria

- [ ] One typed pattern encoder, in the shared builder module str-qwua7.36 introduces, is used by both `instrument.rs` and `analyzer.rs` (parallel-parity rule). No `Const(Str(<pattern text>))` is produced for int literals, ranges, bindings or wildcards.
- [ ] Arm predicates follow Rust's ordered semantics. With `P_j` = pattern of arm j and `G_j` = its guard (true if none), the predicate for arm i is `P_i ∧ G_i ∧ ¬(P_1 ∧ G_1) ∧ … ∧ ¬(P_{i-1} ∧ G_{i-1})`. Int literal → `eq(int)`. Range → `and(ge, le)` / `and(ge, lt)` for `..=` / `..`, including half-open `a..` and `..=b`. Binding → `true` with the binding name substituted by the scrutinee inside the guard. Wildcard → `true` (so it becomes the negation of all earlier arms). Or-patterns → disjunction.
- [ ] Unit tests on the encoder, each asserting a concrete scrutinee value is accepted by exactly the arm Rust would pick:
  - overlapping ranges `0..=10 => A, 5..=15 => B`: 7 → A only; 12 → B.
  - failed guard falls through: `k if k > 100 => A, 50..=200 => B, _ => C`: 150 → A, 60 → B, 10 → C.
  - wildcard after literals and ranges.
  - a proptest that, for generated i64 match arm lists (literals, ranges, guarded bindings, wildcard) and a generated scrutinee value, evaluates the encoded predicates and checks that exactly the arm Rust's first-match rule picks is true.
- [ ] Emitted runtime constraints are validated as the type actually on the wire: the instrumentor emits bare `SymExpr` JSON, and `shatter-rust-runtime/src/lib.rs:178-186` (`branch_hit`) wraps it into `SymConstraint::Expr`. Tests deserialize the emitted string as `protocol::SymExpr` (not `SymConstraint`) and check it contains no `unknown` node.
- [ ] E2E known-answer case in `shatter-core/tests/e2e_concolic_rust.rs`: an integer match whose arms need Z3 (no literal that constant mining could find, e.g. range `1000..=1003` and a guard `k if k * 3 == 6009`), plus a wildcard. The test asserts every arm is reached. Show it failing on main and passing on the branch with `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust <name> -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` skips all cases because they are `#[ignore]`d). Paste both `test result:` lines; each must show 1 run, 0 ignored.
- [ ] After the above is green, retire the core token-text workaround described in `shatter-rust/CLAUDE.md:81-83`, or record in the close note which part must stay (e.g. cross-file `rename_all` enum values) and why, and update that CLAUDE.md section.

## Suggested approach

After str-qwua7.36 lands, extend the shared `build_pattern_sym_expr` for ranges, bindings, guards, or-patterns and wildcards, and have the match visitor fold the earlier-arm negations in order. Keep the per-arm predicate construction in one function that both the analyzer and the instrumentor call.

## Out of scope

- The typed-SymExpr migration, serde escaping and invalid-JSON constraints (str-qwua7.36; audit repros posted there via note `qwua7-36-escaping-repro`).
- Enum/struct patterns beyond what the existing str-mambd workaround handles, unless needed to retire it.
- TS and Go instrumentors.

## Size

M

## References

- Finding frontend-rust-03 (audit 2026-09-22). Old draft: `drafts/shatter-code/59-rust-instrument-constraints.md` (split: the JSON half became note `qwua7-36-escaping-repro`).
- Related: str-qwua7.36 (open, typed SymExpr, blocker), str-mambd (closed, enum-domain token-text workaround).

---

<!-- file: 03-timeout-budget-invariant.md -->

---
slug: timeout-budget-invariant
kind: new
title: "Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout"
priority: P2
type: bug
labels: [timeout, rust-frontend, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [rust-build-deadline-enforcement]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout

## Problem

For a Rust target, the first execute request compiles the harness with cargo and then runs it, all inside one frontend request. The CLI's per-request timeout defaults to 30 s, the same as the CLI-governed build timeout, and shorter than the frontend's own 120 s fallback. So a cold build under load hits the request timeout before the build timeout. The user sees only `request timed out after 30s` with 0 iterations, and nothing points at `--build-timeout` or `--request-timeout`. Closed str-da35 already noted that cold builds needed manual timeout bumps that "should be auto-set". That was never done.

The CLI can only size the request timeout from the build timeout once the frontend actually enforces an aggregate build deadline per request. Today it does not (the build time is checked after cargo exits, and fallback builds each get a fresh budget), which is why this issue is blocked by `rust-build-deadline-enforcement`.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-cli/src/args.rs:561-563`: `--request-timeout`, `default_value_t = 30`. `args.rs:574-577`: `--build-timeout`, `default_value_t = 30`. The same pair recurs at `args.rs:1020/1030`, `1225/1233`, `1339/1347` and `1400/1410` for the other subcommands.
- `PARITY.md:84`: `SHATTER_BUILD_TIMEOUT` / `--build-timeout` governed default `30` s.
- `shatter-rust/src/executor.rs:1017`: `const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 120;` (changed in eeceb7b4). `PARITY.md:96` still says the Rust fallback is 30 s.
- `shatter-rust/src/executor.rs:2990-3010`: the build budget is checked only after blocking `Command::output()` returns (see `rust-build-deadline-enforcement`).
- Audit repro under load (finding frontend-rust-04, not re-run): explore on two trivial Rust functions → `concolic observe failed: frontend error: request timed out after 30s` at 32.7 s with 0 iterations. Re-run with `--request-timeout 180 --build-timeout 170` succeeded.

## Acceptance criteria

- [ ] Enforced invariant in the CLI: for any request that may build (prepare / first execute of a compiled frontend), the effective request timeout is greater than build_timeout + exec_timeout + a fixed margin, where build_timeout is the aggregate per-request deadline the frontend enforces after `rust-build-deadline-enforcement`. Either derive it when the user has not set `--request-timeout`, or give build-capable requests their own budget. If the user sets values that violate it, warn once with both values.
- [ ] Unit test for the invariant covering defaults, user-set build timeout, and user-set request timeout, on every subcommand that takes the pair (`args.rs` sites above) and on both explorer paths (random `explorer.rs` and concolic `orchestrator.rs`) per the parallel-parity rule.
- [ ] A request that times out while a build is in progress produces a diagnostic naming `--build-timeout` / `--request-timeout` (and the existing `SHATTER_BUILD_TIMEOUT` env var), not a bare `request timed out after Ns`. Test: a short request timeout against a frontend stub or fake `cargo` that sleeps, asserting the diagnostic text.
- [ ] `PARITY.md:96` corrected to the real Rust fallback (120 s), and the `cli_parity_tests` in `shatter-cli/src/helpers.rs` still pass.
- [ ] Cold-build proof at close, pasted into the close note:
  - The harness build cache must really be cold. Harness builds ignore the caller's `CARGO_TARGET_DIR` and use `SHATTER_HARNESS_CACHE` (`executor.rs:1062-1073`, `standalone_target_dir`), so set `SHATTER_HARNESS_CACHE` to a fresh empty directory (and a fresh `CARGO_TARGET_DIR` for good measure).
  - Run a Rust explore with default timeout flags and `--timing` (or equivalent) and show the `execute.build` phase with a non-trivial duration or cargo `Compiling` lines, proving a build actually happened, and that the explore completed with iterations > 0.
  - The Rust E2E suite with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste the `test result:` line showing 0 ignored.

## Suggested approach

Compute the request timeout in the CLI wiring from the resolved build and exec timeouts, e.g. `max(request_timeout, build_timeout + exec_timeout + 10)` for prepare/first-execute. Leave the 30 s default for requests that cannot build. Check both explorer paths and every subcommand that takes these flags.

## Out of scope

- Enforcing the build deadline inside the frontend (`rust-build-deadline-enforcement`).
- Reducing cold-build time itself (shared target dirs, dependency prefetch). str-jyxr is not that work: it is about frontend `generate` requests consuming the input-prefetch budget.
- Go/TS timeout defaults, beyond keeping the invariant general.

## Size

S

## References

- Finding frontend-rust-04 (audit 2026-09-22). Old draft: `drafts/shatter-code/60-timeout-budget-invariant.md`.
- Related: str-qe9pp (open; conformance `rust/prepare_supported_rust` times out at the 30 s request timeout, a symptom of this), str-da35 (closed; noted that the values should be auto-set).

---

<!-- file: 04-qwua7-50-panic-boundary.md -->

---
slug: qwua7-50-panic-boundary
kind: note-to-existing
title: "NOTE on str-qwua7.50: poison-tolerant locks cannot help in production because the Rust handler has no panic boundary"
priority: P2
type: bug
labels: [rust-frontend, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.50
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.50: poison-tolerant locks cannot help in production because the Rust handler has no panic boundary

Target: **str-qwua7.50** (open). Action: post the comment below with `bd comments add str-qwua7.50 ...`. Do not change the priority.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-rust-05). This re-scopes the lock half of this issue.
>
> **Current code (re-verified at main 16794cef):**
> - `shatter-rust/src/handler.rs:480` (`Handler::run`) and `:529` (`dispatch`) have no `catch_unwind`, and neither does `shatter-rust/src/main.rs`. The only `catch_unwind` calls are in generated harness code and the generators' native invoke path. A panic while handling a request therefore ends the frontend process (main exits non-zero). No later request in production ever sees a poisoned lock.
> - The cache Mutexes (`executor.rs:406` `CrateHarnessCache`, `:526` `CrateBridgeHarnessCache`, `:733` `HarnessCache`) are only accessed from the dispatch thread. The threads spawned at `executor.rs` ~3055, ~3206, ~5475 only forward child stdout and never touch the caches.
>
> **Correction to the acceptance test as written.** A unit test *can* exercise poison recovery by wrapping a handler call in `catch_unwind` itself and then reusing the handler, so the test is not impossible. But it would pass without fixing anything real, because production cannot survive a panic without a handler-level boundary.
>
> **Proposed re-scope (replaces the poison-tolerant-lock acceptance item):**
> - **Required:** a panic boundary. Wrap `dispatch` in `std::panic::catch_unwind(AssertUnwindSafe(..))`, turn a caught panic into an `internal_error` response naming the panic message, and invalidate (clear) every harness cache the panicking request could have touched. Without this boundary the process dies on the first panic and nothing else in this issue matters.
> - **Then, either** keep the Mutexes and route all 19 lock sites through the `lock_cache` recover-and-clear helper this issue already specifies, **or** replace the Mutexes with owned `HashMap`s on the handler (access is single-threaded). The owned-map option removes poisoning but does not by itself keep the process alive, so it is only acceptable together with the panic boundary above; its cache invalidation happens in the boundary's catch arm.
> - Acceptance: an integration test that spawns **one** frontend process and, over the stdio protocol, sends a request that forces a panic inside dispatch (e.g. a test-only hook or a malformed input known to hit an `expect`), then a normal execute request. The first gets `internal_error` with the panic message, the second succeeds, and the process is still alive afterwards. Show it failing on main (process exits, second request gets no response) and passing on the branch; paste both into the close note.
> - Keep the module-docs half of this issue and the Option::unwrap cleanup unchanged.

---

<!-- file: 05-rust-usize-negative-inputs.md -->

---
slug: rust-usize-negative-inputs
kind: new
title: "usize/u64/u128 params still receive negative integers: int_range() leaves 64- and 128-bit unsigned ints unbounded"
priority: P2
type: bug
labels: [rust-frontend, input-generation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# usize/u64/u128 params still receive negative integers: int_range() leaves 64- and 128-bit unsigned ints unbounded

## Problem

Closed str-ddxe added `int_width` / `int_signed` to `TypeInfo::Int` and in-range generation plus Z3 range assertions for sized ints. But the shared helper every consumer uses, `shatter_core::types::int_range`, deliberately returns `None` for any width whose bounds do not fit in `i64`, which includes `u64`, `u128` and `usize` (and `i128`/`isize`). Random generation, mutation, shrinking, the boundary/candidate path and the solver all treat `None` as "unconstrained full i64". So a `usize` parameter still gets negative values, including `i64::MIN`. The Rust harness rejects them at deserialization, and the report shows the rejections as `throws runtime_error` rows, i.e. as target behavior.

This is a known, documented limitation of the str-ddxe fix (its own doc comment says 64-bit ranges "stay unconstrained"), not a regression. str-ddxe's u8 E2E gate never exercised it. `usize` is the most common Rust integer parameter type, so the gap is large in practice.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-core/src/types.rs:310-345`: `TypeInfo::int_range` / `fn int_range(width, signed)` return `Some` only for 8/16/32-bit widths; the arm `// 64-bit and 128-bit ranges exceed (or fill) i64; leave unconstrained.` returns `None`.
- Consumers that fall back to full i64 on `None`: `shatter-core/src/input_gen.rs:128` (`generate_int`), `:225`, `:1955` (`mutate_int`), `:3576` (`shrink_int`), `:4020`, and `shatter-core/src/solver.rs:170-174` (Z3 range assertions).
- The analyzer maps `usize` correctly: `shatter-rust/src/analyzer.rs:791` `"usize" => Some((64, false))`, `analyzer.rs:1290` `"usize" => int_type(64, false)`.
- Reproduction (audit goals run, finding goals-15). Fixture: `standalone/rust/18_accept_language.rs` in the shatter-examples repo at snapshot `49984f4b974bf937e7e6a98e26a7bc205ddee8e2` (the checkout `scripts/examples_checkout.py` produces), function `fn parse_language_preference(part: &str, order: usize) -> Option<LanguagePreference>` at line 41. The recorded transcript (`audits/2026-09-22/goals-runs/rust-walk.md`, untracked in the audit worktree, lines 150-171) shows 16 paths, 12 of them `throws runtime_error: input 1 deserialization failed: invalid value: integer `-998`, expected usize` with values -998, -44, -1, -644, -838, -9223372036854775808, -690, -926, -301, -945, -16, -905. The exact `shatter` / `shatter-rust` build used was not recorded; an earlier verifier run reported 23 rows / 19 negative with a release build.

## Acceptance criteria

- [ ] A minimal fixture is committed with this issue's branch (not only in the external examples repo): e.g. `fn pick(order: usize) -> u8 { if order > 3 { 1 } else { 0 } }` plus `u64` and `u128` variants. Before changing code, run it with a build of main (record `git rev-parse HEAD` and the `shatter --version` / `shatter-rust` binary path used) and paste the output showing negative inputs.
- [ ] Unsigned 64- and 128-bit ints get a lower bound of 0 everywhere `int_range` is consulted: generation, mutation, shrinking, boundary/candidate seeding and the Z3 range assertion. Either extend the helper's return type (e.g. separate optional min/max) or return `(0, i64::MAX)` for unsigned widths ≥ 64, and document the choice in the helper's doc comment. Signed 64/128-bit stay full range.
- [ ] Proptest over `TypeInfo::Int { int_signed: Some(false), int_width }` for widths 8/16/32/64/128 and the `usize` mapping: every generator, mutator and shrinker entry point, and a solver model under the range assertion, produce values ≥ 0 and within width. The test must fail on main for width 64 (paste the failing output).
- [ ] E2E known-answer test in `shatter-core/tests/e2e_concolic_rust.rs` using the committed fixture asserts zero `deserialization failed` rows for the usize/u64 params. Show it failing on main and passing on the branch with `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust <name> -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste both `test result:` lines.
- [ ] `shatter-llm`'s parser width check (`shatter-llm-parse-validation`) consumes the corrected helper; post a comment on that issue when this lands.

## Suggested approach

Change `int_range` to return `(0, i64::MAX)` for unsigned widths ≥ 64 (values above `i64::MAX` cannot be represented in the current `Value::Number` i64 paths anyway), then audit the five `input_gen.rs` call sites and `solver.rs:170-174` for assumptions that `Some` implies width ≤ 32. Check the TS/Go frontends' use of the same helper (Go `uint64`) per the parity rule.

## Out of scope

- Classifying harness input-deserialization failures as tool errors instead of target throws (`rust-input-deserialize-classification`).
- Values above `i64::MAX` for `u64`/`u128` (representation change).

## Size

S

## References

- Finding goals-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/82-rust-usize-negative-inputs.md`.
- Related: str-ddxe (closed; introduced the helper and the 64-bit exclusion; note `rust-usize-reopen-note`), str-qwua7.14 (closed; walkthrough param-type disagreement), str-4yc9w (open, Go counterpart of the misclassification), str-cfsa (closed, earlier Go counterpart).

---

<!-- file: 06-rust-usize-reopen-note.md -->

---
slug: rust-usize-reopen-note
kind: reopen-note
title: "NOTE on closed str-ddxe: 64/128-bit unsigned ints (incl. usize) were left unbounded and still receive negative integers"
priority: P2
type: bug
labels: [rust-frontend, input-generation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [rust-usize-negative-inputs]
existing_id: str-ddxe
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on closed str-ddxe: 64/128-bit unsigned ints (incl. usize) were left unbounded and still receive negative integers

Target: **str-ddxe** (closed). Action: post the comment below with `bd comments add str-ddxe ...`. Leave str-ddxe closed. The follow-up work is tracked in the new issue `rust-usize-negative-inputs`, which must be filed first so its id can replace `<rust-usize-negative-inputs id>` (that is the only reason for the blocked_by).

## Comment text

> **Audit 2026-09-22 note** (finding goals-15). The fix recorded here deliberately left 64- and 128-bit ints unconstrained: `shatter-core/src/types.rs` `int_range` returns `None` for widths whose bounds do not fit in `i64`, which includes `usize`, `u64` and `u128`, and every generator, mutator, shrinker and the Z3 range assertion treats `None` as full i64 (`input_gen.rs:128,225,1955,3576,4020`, `solver.rs:170-174`). This issue's original report was about `usize` (`score_item`, `history_hits: usize`), so for that type the acceptance ("a fn with a usize param explores non-negative inputs") is not met; the u8 E2E gate did not cover it. Observed in the 2026-09-22 audit: `parse_language_preference(part: &str, order: usize)` (shatter-examples `standalone/rust/18_accept_language.rs`, snapshot 49984f4b) produced 12 of 16 paths as `input 1 deserialization failed: invalid value: integer `-998`, expected usize` (also -44, -1, -644, -838, i64::MIN, ...). Follow-up: **<rust-usize-negative-inputs id>** (lower bound 0 for unsigned ≥ 64-bit on all paths, with a committed fixture).

---

<!-- file: 07-rust-main-duplicate-module-tree.md -->

---
slug: rust-main-duplicate-module-tree
kind: new
title: "shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice"
priority: P2
type: chore
labels: [rust-frontend, tests, performance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice

## Problem

The `shatter-rust` binary declares every module itself (`mod adapters; mod analyzer; ...`) under a crate-wide `#![allow(dead_code)]`, instead of depending on the `shatter_rust` library. So cargo builds two unit-test binaries, `unittests src/lib.rs` and `unittests src/main.rs`, with the same 587 inline tests, and runs both. `rust-fe:test` is the serialized tail of `check-unit`, so this doubles the slowest leaf of the landing gate. The crate-wide `allow(dead_code)` also hides real dead code, and `ENV_LOCK` exists twice. Tests that serialize on it in one binary do not serialize against the other.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/main.rs:3` `#![allow(dead_code)]`, then 12 `mod` declarations from `main.rs:5`. `lib.rs` declares the same modules.
- `ENV_LOCK` is duplicated: `shatter-rust/src/lib.rs:20` and `shatter-rust/src/main.rs:23`.
- `Taskfile.yml:556-562`: `rust-fe:test` runs serialized at the end of `check-unit`, per the comment about nested cargo builds.
- Audit measurement (finding frontend-rust-06): `cargo test --no-run` produced `unittests src/lib.rs` (587 tests via `--list`), `unittests src/main.rs` (587) and `codegen_parity` (4). A prior gate log showed `Starting 1180 tests across 3 binaries ... Summary [208.433s]`. The roughly 50% saving is an estimate.

## Acceptance criteria

- [ ] `main.rs` is a thin binary that uses `shatter_rust::handler::Handler` (or equivalent public entry) and has no `mod` declarations. If that is not possible, `[[bin]] test = false` in `shatter-rust/Cargo.toml`, with the reason recorded.
- [ ] The crate-wide `#![allow(dead_code)]` is removed. Real dead code it was hiding is deleted, or allowed item-by-item with a reason.
- [ ] One `ENV_LOCK`.
- [ ] Proof at close: `cargo nextest list -p shatter-rust` (or `cargo test -p shatter-rust -- --list`) before and after, showing the unit test count halved, and `task rust-fe:test` wall time before and after, forced to execute (not a checksum-cached no-op; see the project gate-cache note). Paste both into the close note.
- [ ] `task rust-fe:test` passes, and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing because every case is `#[ignore]`d). Paste the `test result:` line, which must show 0 ignored.

## Suggested approach

Make any items the binary needs `pub` (or `pub(crate)` re-exported through a small `pub mod` surface) and change `main.rs` to `fn main() { shatter_rust::run() }`-style. Check that `codegen_parity.rs` and other integration tests do not rely on binary-only items.

## Out of scope

- Splitting the executor module or reorganizing tests.
- Other crates' test layout.

## Size

S

## References

- Finding frontend-rust-06 (audit 2026-09-22). Old draft: `drafts/shatter-code/62-rust-main-duplicate-module-tree.md`.

---

<!-- file: 08-rust-tests-offline-silent-pass.md -->

---
slug: rust-tests-offline-silent-pass
kind: new
title: "shatter-rust tests early-return as passed when a fixture build fails with an offline-looking (or any cargo-mentioning) error"
priority: P2
type: bug
labels: [rust-frontend, tests, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust tests early-return as passed when a fixture build fails with an offline-looking (or any cargo-mentioning) error

## Problem

Many shatter-rust executor tests build fixture crates. When the build fails with an error that matches one of two predicates, the test prints `skipping ...` to stderr and returns, and nextest records that as a pass. The nextest profile has `status-level = "fail"`, so the skip message is never shown. A CI run or sandbox without network (or with a cold registry cache) can therefore report `rust-fe:test` green while testing none of the crate-building paths.

The second predicate is much broader than "offline": `cargo_build_unavailable` accepts any error message containing the substring `cargo` or `No such file`. Most real compile failures of a fixture harness mention cargo (paths, `failed to run cargo`, cargo's own output), so some tests may silently pass on genuine regressions, not only when offline.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-rust/src/executor.rs:7611` `fn is_offline_compile_error_message(msg)` matches messages such as `spurious network error` and `Could not resolve host`. It guards 15 early-return arms of the form `Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => { eprintln!("skipping ..."); }` (`executor.rs:8088, 8276, 8317, 8432, 8526, 8627, 8745, 8794, 8902, 8966, 9033, 9093, 9164, 10178, 10588`).
- `executor.rs:11566-11573` `fn cargo_build_unavailable(msg)` returns true if `msg.contains("cargo") || msg.contains("No such file") || ...network strings`. It guards 18 more early-return arms (`executor.rs:7830, 7909, 11611, 11687, 11758, 11857, 12463, 12606, 12672, 12710, 12748, 12842, 12953, 13035, 13124, 13185, 13427, 13438`).
- `shatter-rust/src/handler.rs:1506` and `:2027` add two more `skipping ...: cargo unavailable` early returns in handler tests (the `analyzer.rs:868` hit is only a doc comment). The 38 textual `skipping` occurrences in `executor.rs` are not a count of distinct affected tests.
- `.config/nextest-standalone.toml:12` and `:20`: `status-level = "fail"` (with `final-status-level = "slow"` / `"flaky"`), so passing tests' stderr, including the skip line, is not shown.

## Acceptance criteria

- [ ] Inventory first, recorded in the close note as a table: every test in `shatter-rust/src/**` and `shatter-rust/tests/**` that can return early without asserting (the 33 predicate-guarded arms above, the two `handler.rs` skips, and any other early `return` on an environment condition), with test name and guard.
- [ ] `cargo_build_unavailable`'s bare `"cargo"` and `"No such file"` substrings are removed. A genuine fixture compile error (e.g. a deliberate type error in the fixture) fails the test. Unit test: `cargo_build_unavailable("error[E0308]: mismatched types ... cargo build failed")` is false.
- [ ] Every remaining environment skip goes through one helper that, under `CI` (or a `SHATTER_REQUIRE_NETWORK_TESTS=1` switch the gate sets), panics instead of returning, and otherwise prints a single fixed marker line (e.g. `SHATTER-TEST-SKIP: <test> <reason>`).
- [ ] `rust-fe:test` reports the number of skip markers and fails in CI when it is non-zero. The CI workflow prefetches the fixture crates' dependencies itself (e.g. `cargo fetch --manifest-path` for each fixture, or a warmed `CARGO_HOME` cache step); str-jyxr is unrelated (it is about frontend `generate` requests and the input-prefetch budget) and provides no fixture prefetch.
- [ ] Offline proof at close that reaches the early-return branch rather than failing earlier. An empty `CARGO_HOME` plus offline mode can prevent the outer test crate from compiling at all, which proves nothing. Instead:
  1. Build the test binaries normally: `cargo nextest archive -p shatter-rust --archive-file /tmp/rfe.tar.zst` (or `cargo test -p shatter-rust --no-run` and note the binary path).
  2. Run them with fixture builds forced offline: `CARGO_NET_OFFLINE=true CARGO_HOME=$(mktemp -d) SHATTER_HARNESS_CACHE=$(mktemp -d)` and the archived/prebuilt binary.
  3. Show, on main, the tests reported as passed while stderr (`--no-capture` or `--success-output immediate`) contains the skip lines, i.e. the early-return branch was reached; and on the branch, the same run reporting those tests as failed (with `CI=1`) or explicitly counted as skipped.
  4. One normal networked run showing 0 skip markers.
  Paste all outputs into the close note.

## Suggested approach

Replace both predicates' 33 guards with one helper, e.g. `fn env_skip_or_fail(test: &str, msg: &str)` that matches only well-known network/registry failure strings and panics when `CI` is set. Better still, move the network-dependent tests into a nextest test group that an explicit offline profile excludes, so skips are visible as skips.

## Out of scope

- General nextest profile changes (see str-ed38.3 and the test-hygiene issues in this epic).
- Reducing how many tests build fixture crates.

## Size

M

## References

- Finding frontend-rust-11 (audit 2026-09-22). Old draft: `drafts/shatter-code/63-rust-tests-offline-silent-pass.md`.

---

<!-- file: 09-rust-frontend-design-dedupe.md -->

---
slug: rust-frontend-design-dedupe
kind: new
title: "shatter-rust rebuilds the crate type registry on every analyze (about N² file parses per crate scan)"
priority: P3
type: refactor
labels: [rust-frontend, performance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust rebuilds the crate type registry on every analyze (about N² file parses per crate scan)

## Problem

Every analyze call rebuilds the crate type registry. It walks the crate's `src/` and `syn`-parses every `.rs` file, with no cache on the `Handler`. Scanning N files of one crate costs about N×N file parses (inferred, not measured).

## Evidence

Re-verified against the audit worktree at commit 56c86168 (no change in `shatter-rust/` at main 16794cef):

- `shatter-rust/src/analyzer.rs:83` and `:201` both call `build_crate_type_registry(file_path)`, defined at `analyzer.rs:907`.

## Acceptance criteria

- [ ] Before changing code, measure: `shatter scan` wall time and the number of `build_crate_type_registry` file parses (temporary counter or `--timing` phase) on a multi-file Rust crate (name the crate and commit, e.g. an examples crate). Record in the close note.
- [ ] The crate type registry is cached on the `Handler`, keyed by crate root. Invalidation uses per-file change detection: the cache stores, for every `.rs` file it parsed, the path plus (size, mtime) or a content hash, and is rebuilt when any stored file changed, was removed or replaced, or when a new `.rs` file appears under the crate's source roots. An aggregate fingerprint such as max-mtime + file count is not acceptable: it misses an edit to an older file while a newer file keeps the max, and a replace that keeps the count.
- [ ] Tests: (1) a second analyze in the same crate does not re-parse (parse counter unchanged); (2) editing a file that is not the newest invalidates; (3) replacing a file with same-size different content invalidates (use a content hash or ensure mtime moves); (4) adding a file invalidates; (5) removing a file invalidates.
- [ ] The same scan re-measured after the change, with parse count and wall time in the close note.
- [ ] `task rust-fe:test` passes and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste the `test result:` line with 0 ignored. Protocol output is unchanged, so no parity-contract update is expected.

## Suggested approach

Keep a `HashMap<PathBuf, (Vec<(PathBuf, u64, SystemTime, u64 /*hash*/)>, Arc<CrateTypeRegistry>)>` on the handler and pass the `Arc` into the analyze functions. Re-stat the file list on each lookup (cheap compared with re-parsing).

## Out of scope

- Unifying the two Axum extractor classifiers (`rust-axum-extractor-classifier-dedupe`).
- Cross-crate / multi-file analysis beyond caching.

## Size

S

## References

- Finding frontend-rust-12 (audit 2026-09-22). Old draft: `drafts/shatter-code/64-rust-frontend-design-dedupe.md` (split: the extractor half is `rust-axum-extractor-classifier-dedupe`).

---

<!-- file: 10-rust-runtime-harness-loop.md -->

---
slug: rust-runtime-harness-loop
kind: new
title: "Rust harness loops (runtime and generated crate-bridge driver) swallow malformed requests; branches on user-spawned threads are silently lost"
priority: P3
type: bug
labels: [rust-frontend, runtime, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust harness loops (runtime and generated crate-bridge driver) swallow malformed requests; branches on user-spawned threads are silently lost

## Problem

1. **Malformed requests become empty inputs.** The runtime harness loop parses each request line with `serde_json::from_str(line).unwrap_or_default()`. An unparseable request turns into `Value::Null`, the target runs with empty inputs, and the result is a misleading `input 0 deserialization failed` instead of a protocol error. The generated crate-bridge driver loop in `shatter-rust/src/executor.rs` has the same pattern (it also defaults a missing `function` to `""`), so the behavior depends on which harness a target is routed to. Both loops are in scope.
2. **Unescaped fallback JSON.** `flush_results`' serialization-error fallback interpolates the error text into a JSON string with `format!` and does not escape it. An error message containing `"` or `\` produces invalid JSON.
3. **Thread-local branch tracking.** Branch/coverage state is `thread_local!`, so branches executed on threads spawned by user code (`std::thread::spawn`, rayon, `tokio::task::spawn_blocking`) are silently dropped. Closed str-dfnu2 / str-oc67 fixed only tokio cross-worker awaits (current-thread runtime). Neither `shatter-rust/CLAUDE.md` nor the parity matrix documents the remaining limitation.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust-runtime/src/lib.rs:558` and `:612`: `let req: Value = serde_json::from_str(line).unwrap_or_default();`, followed by `req["inputs"].as_array().cloned().unwrap_or_default()` (`:559`, `:614`). Similar `unwrap_or_default()` parsing at `:229`, `:256` (args) and `:358` (mocks).
- `lib.rs:340-345` (`flush_results`, defined at `:318`): `format!(r#"{{..."message":"{}"...}}"#, e)`, with no escaping.
- `lib.rs:167`: `thread_local! {` holds STATE.
- Parallel path: `shatter-rust/src/executor.rs:5179-5182`, the generated crate-bridge driver loop, emits `let req: Value = serde_json::from_str(line).unwrap_or_default();`, `req["function"].as_str().unwrap_or("")` and `req["inputs"].as_array().cloned().unwrap_or_default()`.

## Acceptance criteria

- [ ] In both loops (runtime `lib.rs:558`/`:612` and the generated crate-bridge driver at `executor.rs:5179-5182`), an unparseable request line, a request missing `inputs`, and (crate-bridge) a request missing `function` produce an explicit protocol-error execute result that names the parse error. The target is not invoked. Tests for each case in each loop, shown failing on main; the crate-bridge case runs a generated driver (existing crate-bridge test harness) and sends the malformed line over stdin.
- [ ] The `flush_results` fallback is built with `serde_json::json!` (or equivalent). Unit test with an error message containing `"` and `\` asserts the output parses.
- [ ] The args/mocks `unwrap_or_default()` sites either get the same explicit error or carry a comment explaining why a default is correct.
- [ ] Thread limitation: either document it in `shatter-rust/CLAUDE.md` and `protocol/parity-matrix.yaml` (then run `task parity`), or implement a process-global recorder keyed by execution id, with a test where a branch on a `std::thread::spawn` thread is recorded.
- [ ] `cargo test -p shatter-rust-runtime` and `task rust-fe:test` pass, and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste the `test result:` line with 0 ignored.

## Suggested approach

Decode into a typed `struct Request { inputs: Vec<Value> }` with `serde_json::from_str::<Request>` and match the error into an `ExecuteResult` with `thrown_error.error_type = "protocol_error"`. Documenting the thread limitation is the cheap first step. A global recorder needs care with concurrent executions and should only be done if a real target needs it.

## Out of scope

- The crate-bridge stdout channel (`rust-crate-bridge-stdout`); if both land close together, the malformed-request change goes on top of whichever lands first in the same generator function.
- Async runtime flavor changes.

## Size

S

## References

- Finding frontend-rust-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/65-rust-runtime-harness-loop.md`.
- Related: str-dfnu2, str-oc67 (closed; tokio cross-worker fix).

---

<!-- file: 11-shatter-llm-hardening.md -->

---
slug: shatter-llm-hardening
kind: new
title: "LLM API keys are reachable through derived Debug (JevConfig, adapter configs, ExploreConfig.frontier_ranker)"
priority: P3
type: bug
labels: [llm, security, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# LLM API keys are reachable through derived Debug (JevConfig, adapter configs, ExploreConfig.frontier_ranker)

## Problem

`JevConfig` holds `api_key: String` and derives `Debug`. The containing types (`JevAdapter`, `ReplayDecisionOracle`, `DecisionFrontierRanker`) also derive Debug, and `DecisionFrontierRanker` ends up as `ExploreConfig.frontier_ranker`, with `ExploreConfig` deriving Debug too. The other adapters (anthropic, openai, google) keep `api_key: String` in their structs, and `shatter-core/src/config.rs` holds `api_key: Option<String>` in `#[derive(Debug)]` LLM config sections. A future `{:?}` of the explore config, an adapter, or the loaded config would print the key. No current log site is known to print it.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/jev.rs:23` `#[derive(Debug, Clone)]` on `JevConfig`, with `pub api_key: String` at `:26`. `jev.rs:47` `#[derive(Debug)]` (JevAdapter). `shatter-llm/src/replay.rs:12` and `shatter-llm/src/decision_ranker.rs:18` `#[derive(Debug)]`.
- `shatter-llm/src/anthropic.rs:21`, `openai.rs:21`, `google.rs:19`: `api_key: String`.
- `shatter-core/src/config.rs:325, 357, 388`: `pub api_key: Option<String>` in the anthropic/openai/google adapter config sections.
- `shatter-core/src/orchestrator.rs:102` `#[derive(Debug, Clone)]` on `ExploreConfig`, with `frontier_ranker: Arc<dyn FrontierRanker>` at `:164`.
- Dependency direction: `shatter-llm/Cargo.toml:8` depends on `shatter-core`; core has `shatter-llm` only as a dev-dependency (`shatter-core/Cargo.toml:44`, itself slated for removal by str-qwua7.43). A shared secret type therefore cannot live in `shatter-llm` if `shatter-core::config` is to use it.

## Acceptance criteria

- [ ] Every struct that holds an LLM API key (Jev, anthropic, openai, google, custom/local if they hold one, and the `shatter-core/src/config.rs` sections) prints a redacted value under `Debug`. Either a redacting newtype defined in `shatter-core` (or a new dependency-free crate both can use), or manual `Debug` impls. The type must not be defined in `shatter-llm` and imported into `shatter-core` (that would reverse the production dependency).
- [ ] Serialization of config files is unchanged (keys still round-trip through serde where they did before); a round-trip test covers one config section.
- [ ] Unit tests with a sentinel key (e.g. `sk-test-SENTINEL`) assert `format!("{:?}", x)` does not contain it for each config/adapter type above, for `DecisionFrontierRanker`, and for an `ExploreConfig` holding a ranker built from a keyed config. Show at least the `JevConfig` and `ExploreConfig` tests failing on main.
- [ ] `cargo test -p shatter-llm -p shatter-core` passes.

## Out of scope

- Parser type validation (`shatter-llm-parse-validation`) and retry backoff (`shatter-llm-backoff-cap`).
- User documentation for the LLM oracle (note on str-qwua7.21, slug `qwua7-21-llm-seed-oracle-docs`).
- The core→shatter-llm dev-dependency cycle (str-qwua7.43).

## Size

S

## References

- Finding frontend-rust-16 (audit 2026-09-22). Old draft: `drafts/shatter-code/66-shatter-llm-hardening.md` (split after the Codex cross-check into this issue, `shatter-llm-parse-validation` and `shatter-llm-backoff-cap`).
- Related: str-dcgk / str-m0ta (closed; built the crate).

---

<!-- file: 12-qwua7-21-llm-seed-oracle-docs.md -->

---
slug: qwua7-21-llm-seed-oracle-docs
kind: note-to-existing
title: "NOTE on str-qwua7.21: add an LLM seed-oracle user guide and a default-model-ID update policy"
priority: P3
type: task
labels: [docs, llm, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.21
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.21: add an LLM seed-oracle user guide and a default-model-ID update policy

Target: **str-qwua7.21** (open user-docs epic). Action: post the comment below with `bd comments add str-qwua7.21 ...`. If the maintainer prefers a tracked child, file the "Proposed child" block as a new issue with `--parent str-qwua7.21` (P3, type task, labels docs,llm) instead of, or as well as, the comment. There is no old draft for this item; it comes from report §15.1 and finding frontend-rust-18.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-rust-18). The LLM seed oracle has no user documentation, and its default model IDs are hard-coded without an update policy. Neither .21.1 (config reference, which mentions LLM config keys) nor str-qwua7.20.2 (SHATTER_* env-var table) covers this.
>
> **Current state (re-verified at 56c86168):**
> - The only user-facing mention is `SPEC.md:201`: "`explore` also accepts LLM seed-oracle overrides (`--llm`, `--llm-adapter`, `--llm-token-budget`)." `README.md` and `QUICKSTART.md` do not mention the LLM oracle or `SHATTER_ANTHROPIC_API_KEY`.
> - Adapters (anthropic/openai/google/custom/local), config keys (`llm.anthropic.api_key`, `llm.custom`, `llm.local`), `SHATTER_ANTHROPIC_API_KEY`, token budgets, and the fact that **target source code is sent to third-party APIs** are described only in `docs/superpowers` design specs.
> - Default models are hard-coded in `shatter-core/src/config.rs:338` (`claude-sonnet-4-6`), `:376` (`gpt-4o`), `:392` (`gemini-2.0-flash`). Nothing records when or how they are refreshed. The adapter list (anthropic/openai/google/custom/local) is documented only in a doc comment at `shatter-cli/src/helpers.rs:1598-1605` (`build_oracle_adapter`).
>
> **Proposed child (acceptance criteria):**
> - `docs/llm-oracle.md` user guide: what the oracle does and when it helps; enabling it (`--llm`, `--llm-adapter`, `--llm-token-budget`, config keys, env vars); **data egress** (what source/context is sent to which provider); cost controls (token budget, replay/cache); local/custom adapters for no-egress use. Linked from README and QUICKSTART.
> - A written default-model refresh policy (who updates the IDs in `config.rs`, when, and how the change is tested), placed next to the defaults or in the guide.
> - Proof at close: the guide is linked from README and QUICKSTART (paste the link lines), and every flag, config key and env var it names exists (`shatter explore --help` output and a `grep` of `shatter-core/src/config.rs` pasted).
>
> Runtime diagnostics for a provider rejecting a retired model are tracked separately (audit slug `llm-model-rejection-diagnostic`), not in this docs work.

---

<!-- file: 13-qwua7-36-escaping-repro.md -->

---
slug: qwua7-36-escaping-repro
kind: note-to-existing
title: "NOTE on str-qwua7.36: audit repros for invalid hand-formatted constraint JSON (`1.` floats, control chars) and silent Unknown"
priority: P2
type: bug
labels: [rust-frontend, instrumentation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.36
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.36: audit repros for invalid hand-formatted constraint JSON (`1.` floats, control chars) and silent Unknown

Target: **str-qwua7.36** (open, "build runtime constraints as typed protocol::SymExpr instead of hand-formatted JSON"). Action: post the comment below with `bd comments add str-qwua7.36 ...`. Do not change priority or scope ownership; str-qwua7.36 already owns the typed builder, serde serialization, deletion of `escape_json_string` and the proptest. The new match-arm semantics issue (`rust-instrument-constraints`) is blocked by str-qwua7.36.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-rust-02). Two concrete repros for this issue's "second place to get escaping/shape wrong" point, plus one gap the current acceptance does not cover.
>
> **Repros (code re-verified at main 16794cef):**
> - `shatter-rust/src/instrument.rs:671-702` `constraint_for_lit`: float literals are emitted with `f.base10_digits()` raw (`:682`), so `if x > 1. {}` produces `"value":1.`, which is not valid JSON.
> - `instrument.rs:704` `escape_json_string` escapes only `\\`, `"`, `\n`, `\r`, `\t`, so `if s == "a\u{1}b" {}` embeds a raw U+0001 in the JSON string, which is invalid JSON.
> - Audit probe (not re-run): an explore artifact listed the `x > 1.` and `s == "a\u{1}b"` constraints as kind `unknown`, and concolic explore never reached `if s == "a\u{1}b"` in 60 iterations. The analyzer's typed builder emits `1.0` and `"a\u0001b"` correctly for the same source.
>
> **Please add to acceptance:**
> - The proptest over generated conditions includes float literals written `1.`, `1e10`, `1.5e-3`, and string literals with arbitrary chars including U+0000-U+001F. It deserializes the emitted string as `protocol::SymExpr` (the instrumentor emits bare `SymExpr`; `shatter-rust-runtime/src/lib.rs:178-186` `branch_hit` wraps it into `SymConstraint::Expr`), not as `SymConstraint`.
> - Silent `Unknown`: `branch_hit` (`shatter-rust-runtime/src/lib.rs:178-186`) turns any unparseable constraint into `SymConstraint::Unknown { hint }` with no warning or counter. Make that observable (a counter in the execute result / telemetry, or a warning on stderr that the frontend surfaces), with a unit test that feeds `{"value":1.}` and asserts the counter/warning. If you prefer to keep this issue narrow, say so here and the audit will file it separately.
> - E2E: an `e2e_concolic_rust` case that reaches `if s == "a\u{1}b"` via a runtime constraint. Note that plain `cargo test --test e2e_concolic_rust` runs nothing (all cases are `#[ignore]`d); use the `task e2e-rust-governed` command with `-- --include-ignored` and paste the `test result:` line.

---

<!-- file: 14-rust-build-deadline-enforcement.md -->

---
slug: rust-build-deadline-enforcement
kind: new
title: "shatter-rust build timeout is checked only after cargo exits, and fallback builds each get a fresh budget: no build deadline is actually enforced"
priority: P2
type: bug
labels: [timeout, rust-frontend, harness, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust build timeout is checked only after cargo exits, and fallback builds each get a fresh budget: no build deadline is actually enforced

## Problem

The Rust frontend's build timeout (`--build-timeout` / `SHATTER_BUILD_TIMEOUT`, frontend fallback 120 s) is not a deadline. The harness build runs `cargo build` with blocking `Command::output()` and compares elapsed time to the budget only after cargo has exited. A slow or hung build therefore runs until cargo finishes, however long that takes, and only then turns into a `CompilationFailed` "timed out". A single execute request can also run several builds in sequence (bin-only then crate-bridge routing; whole-file crate-bridge build, then per-candidate and single-function fallbacks), and each build gets the full budget again.

As a result the CLI cannot size its per-request timeout from the build timeout (issue `timeout-budget-invariant`): the real worst-case build time for one request is unbounded.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-rust/src/executor.rs:2990-3010`: `Command::new("cargo").args(&cargo_args)....output()` (blocking), then `if build_start.elapsed() > build_timeout { return Err(CompilationFailed(..)) }`. No child kill, no wait-with-timeout. The optional `cargo_check_before_build` (`:2988`) runs before it with its own blocking call.
- `executor.rs:1017`: `const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 120;`.
- Multiple builds per request: `executor.rs:5535-5861` (crate-bridge whole-file build degrading to per-candidate and single-function fallbacks, see the comments at `:5535`, `:5747`, `:5755`, `:5861`) and `executor.rs:6292-6300` (bin-only first, crate-bridge fallback).

## Acceptance criteria

- [ ] Every cargo invocation the frontend makes for a harness (check, build, fallback builds) runs as a spawned child that is killed when its deadline passes (e.g. `spawn()` + a wait-with-timeout loop or a watchdog thread, with the child's process group killed so rustc children die too). Output is still captured for diagnostics.
- [ ] One aggregate build deadline per execute/prepare request: all builds the request triggers (routing fallback, whole-file, per-candidate, single-function) share the remaining budget instead of each getting a fresh one. When the budget runs out mid-sequence, the request fails with a `CompilationFailed` message that says the build budget was exhausted, names the budget value and `--build-timeout`, and says how many builds were attempted.
- [ ] Test (red on main, green on the branch; paste both into the close note): with a fake `cargo` on `PATH` that sleeps 30 s, and a 2 s build timeout, an execute request returns the budget-exhausted error in under 5 s and leaves no `cargo`/sleep child running.
- [ ] Test: a request whose first build fails fast and whose fallback build is slow is bounded by the one aggregate deadline, not two.
- [ ] `task rust-fe:test` passes, and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing because every case is `#[ignore]`d). Paste the `test result:` line, which must show 0 ignored.

## Suggested approach

Wrap the build commands in one helper `run_cargo_with_deadline(cmd, deadline: Instant)` that spawns, drains stdout/stderr on threads, polls `try_wait`, and kills the process group at the deadline. Thread a `deadline: Instant` computed once per request through the build/fallback functions instead of a `Duration` per call.

## Out of scope

- CLI-side request-timeout sizing and the timeout diagnostic (`timeout-budget-invariant`, blocked by this issue).
- Making builds faster (target-dir sharing, prefetch).

## Size

M

## References

- Split from `timeout-budget-invariant` after the Codex cross-check of the 2026-09-22 audit (finding frontend-rust-04).
- Related: str-da35 (closed; cold builds needed manual timeout bumps).

---

<!-- file: 15-rust-input-deserialize-classification.md -->

---
slug: rust-input-deserialize-classification
kind: new
title: "Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors"
priority: P2
type: bug
labels: [rust-frontend, reporting, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors

## Problem

When the Rust harness cannot deserialize an input into the parameter's type (`input N deserialization failed: invalid value: integer `-998`, expected usize`), the target function never runs. Explore/scan output nevertheless records the row as `throws runtime_error: ...`, which reads as a behavior of the target. Any input the generator gets wrong (see `rust-usize-negative-inputs`, but also any future type mismatch) becomes a fake finding, and the walkthrough error regex does not match "deserialization failed", so no gate notices.

## Evidence

- Audit transcript `audits/2026-09-22/goals-runs/rust-walk.md` lines 159-171 (untracked in the audit worktree): 12 of 16 `parse_language_preference` rows are `throws runtime_error: input 1 deserialization failed: ...`.
- Go has the same class of problem tracked as str-4yc9w (open); str-cfsa (closed) was an earlier Go counterpart.

## Acceptance criteria

- [ ] Locate where the Rust harness produces the `input N deserialization failed` error (generated harness code in `shatter-rust/src/executor.rs` / `shatter-rust-runtime`) and give it a distinct `thrown_error.error_type` (e.g. `input_error`) or a distinct execute-result outcome, consistent with whatever str-4yc9w chooses for Go. Name the sites in the close note.
- [ ] The core and report layers treat that outcome as a tool/input error: it is not counted as a target behavior/finding in explore and scan output, and it is counted in the run's error summary.
- [ ] Test: a harness-level unit test feeding a negative integer to a `usize` param asserts the new classification, and a report-level test asserts the row is not rendered as `throws`. Show them failing on main.
- [ ] If the classification is protocol-visible (new `error_type` value), update `protocol/parity-matrix.yaml` and `shatter-rust/CLAUDE.md`, and run `task parity` + `task conformance`.

## Out of scope

- Fixing the generator bug that currently produces most of these rows (`rust-usize-negative-inputs`).
- The Go-side change (str-4yc9w), except for agreeing on the shared classification.

## Size

S

## References

- Split from `rust-usize-negative-inputs` after the Codex cross-check of the 2026-09-22 audit (finding goals-15).
- Related: str-4yc9w (open, Go), str-cfsa (closed, Go).

---

<!-- file: 16-rust-axum-extractor-classifier-dedupe.md -->

---
slug: rust-axum-extractor-classifier-dedupe
kind: new
title: "shatter-rust has two independent Axum extractor classifiers (adapters.rs 12 kinds, executor.rs 5 kinds) that can disagree"
priority: P3
type: refactor
labels: [rust-frontend, axum, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust has two independent Axum extractor classifiers (adapters.rs 12 kinds, executor.rs 5 kinds) that can disagree

## Problem

`adapters.rs` classifies Axum extractors (12 kinds, keyed by `ParamInfo.type_name`) for the adapter path. `executor.rs` has a separate 5-kind classifier (Path/Query/Json/State/Multipart) that re-parses type strings with `syn` for the generic wrappers. They can disagree, and support added to one is missing from the other.

## Evidence

Re-verified against the audit worktree at commit 56c86168 (no change in `shatter-rust/` at main 16794cef):

- `shatter-rust/src/adapters.rs:389` `pub enum AxumExtractorKind`.
- `shatter-rust/src/executor.rs:1796` `enum AxumExtractor`, used around `executor.rs:2167, 2195, 2222, 2452, 2776, 4989, 6571, 6693` (per the audit; re-check exact sites when implementing).

## Acceptance criteria

- [ ] Before refactoring, a table test runs every type string in `AXUM_EXTRACTOR_TYPES` (plus wrapped forms such as `Path<(u32, String)>`, `Json<Foo>`, `State<Arc<AppState>>`) through both classifiers and records where they disagree. Paste the disagreement list in the close note (it may be empty).
- [ ] One classifier in `adapters.rs` returns kind + inner type, and `executor.rs` consumes it; the `executor.rs` enum is deleted.
- [ ] The table test then asserts the single classifier's result for every entry.
- [ ] Existing Axum tests pass (`task rust-fe:test`), and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored`; paste the `test result:` line with 0 ignored. If execute output changes for any extractor, update `protocol/parity-matrix.yaml` and run `task parity` + `task conformance`.

## Out of scope

- Adding support for new extractor kinds (str-la75, str-62pj, str-38in cover individual extractors).
- Registry caching (`rust-frontend-design-dedupe`).

## Size

S

## References

- Finding frontend-rust-14 (audit 2026-09-22). Split from `rust-frontend-design-dedupe` after the Codex cross-check.

---

<!-- file: 17-shatter-llm-parse-validation.md -->

---
slug: shatter-llm-parse-validation
kind: new
title: "shatter-llm response parser ignores int width/signedness and tries only the first '[' in model output; no property tests"
priority: P3
type: bug
labels: [llm, input-generation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [rust-usize-negative-inputs]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-llm response parser ignores int width/signedness and tries only the first '[' in model output; no property tests

## Problem

`shatter-llm/src/parse.rs` turns untrusted model output into seed inputs.

1. `type_matches` accepts any integer for `TypeInfo::Int { .. }` regardless of `int_width` / `int_signed` (-1 passes for `u8`, 300 passes for `u8`), so LLM seeds can carry the same out-of-range values that the Rust harness rejects at deserialization.
2. `extract_first_json_array` tries only the first `[` in the model output, so prose such as "[note] ... [{...}]" loses the real array.
3. The module has only example tests (about 10) and no property tests.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/parse.rs:104` `TypeInfo::Int { .. } => v.is_i64() || v.is_u64(),`. `parse.rs:62` `fn extract_first_json_array`.
- The core range helper `shatter_core::types::int_range` (`shatter-core/src/types.rs:310-345`) returns `None` for 64/128-bit widths including `usize`, so reusing it unchanged would still accept negatives for `usize`/`u64`. `rust-usize-negative-inputs` fixes the helper; this issue is blocked by it so the parser reuses the corrected version.

## Acceptance criteria

- [ ] Integer values are validated with the corrected core helper (after `rust-usize-negative-inputs`): unsigned rejects negatives at every width including 64/128/usize, and 8/16/32-bit bounds are enforced. Unit tests: u8 -1 and 256 rejected, i8 -129 rejected, usize -1 rejected, u64 0 accepted. Show them failing on main.
- [ ] `extract_first_json_array` tries successive `[` candidates until one parses as the expected array shape. Unit test with a leading bracketed prose fragment, and one where no candidate parses.
- [ ] Proptest in `parse.rs`: for arbitrary strings and arbitrary `ParamInfo` type lists, `parse_response` never panics and returns only vectors whose values conform to the declared types (including width/sign).
- [ ] `cargo test -p shatter-llm` passes.

## Out of scope

- API-key redaction (`shatter-llm-hardening`) and retry backoff (`shatter-llm-backoff-cap`).

## Size

S

## References

- Finding frontend-rust-17 (audit 2026-09-22). Split from `shatter-llm-hardening` after the Codex cross-check.
- Related: str-qwua7.47 (PBT for core modules only).

---

<!-- file: 18-shatter-llm-backoff-cap.md -->

---
slug: shatter-llm-backoff-cap
kind: new
title: "shatter-llm retry backoff is uncapped: `100ms << attempt` overflows at attempt 64 and Retry-After is honored without a ceiling"
priority: P3
type: bug
labels: [llm, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-llm retry backoff is uncapped: `100ms << attempt` overflows at attempt 64 and Retry-After is honored without a ceiling

## Problem

`RateLimitedOracle` retries with `Duration::from_millis(100u64 << attempt)` and no cap. The delay reaches minutes by attempt 11 and hours by attempt 16, and the shift overflows at attempt 64 (a panic in debug builds). A server-provided Retry-After is also used uncapped, so one bad header can stall an explore run indefinitely. The number of attempts is bounded only by `max_retries`, which is user configuration (`LlmConfig.max_retries`), so large values are reachable.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/rate_limit.rs:61-62` `retry_after.unwrap_or_else(|| Duration::from_millis(100u64 << attempt))`, bounded only by `max_retries` (`:30`, `:58`).
- `shatter-llm/src/registry.rs:28-50` and `shatter-cli/src/helpers.rs:1612` pass `config.max_retries` straight through.

## Acceptance criteria

- [ ] Computed backoff is capped (e.g. `min(100ms · 2^n, 30s)` using `checked_shl` / saturating math), and Retry-After is clamped to the same ceiling. The ceiling is a named constant documented in the module docs.
- [ ] Unit tests: attempt 63 and 64 do not panic and return the cap; a Retry-After of 1 hour is clamped. Use a mock clock or assert on the computed `Duration` rather than sleeping. Show the attempt-64 test failing (panicking) on main in a debug build.
- [ ] `cargo test -p shatter-llm` passes.

## Out of scope

- API-key redaction (`shatter-llm-hardening`) and parser validation (`shatter-llm-parse-validation`).

## Size

XS

## References

- Finding frontend-rust-16 (audit 2026-09-22). Split from `shatter-llm-hardening` after the Codex cross-check.

---

<!-- file: 19-llm-model-rejection-diagnostic.md -->

---
slug: llm-model-rejection-diagnostic
kind: new
title: "Reproduce and fix how a rejected/retired LLM model surfaces: Anthropic adapter reports a generic `HTTP 404`, CLI propagation unverified"
priority: P3
type: bug
labels: [llm, cli, diagnostics, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reproduce and fix how a rejected/retired LLM model surfaces: Anthropic adapter reports a generic `HTTP 404`, CLI propagation unverified

## Problem

Default model IDs are hard-coded in `shatter-core/src/config.rs` (`claude-sonnet-4-6`, `gpt-4o`, `gemini-2.0-flash`) and will eventually be retired by the providers. The OpenAI and Google adapters already turn a 404 into a specific "model not found" error. The Anthropic adapter does not: a 404 falls through to the generic `Anthropic API HTTP {status}: {text}`. It is also unverified what the user sees when any adapter fails this way during explore: whether the error reaches the CLI output, is logged as a warning, or is swallowed while the run silently continues without the oracle.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/openai.rs:132-135`: `NOT_FOUND` → `OpenAI model not found (404): {text}`.
- `shatter-llm/src/google.rs:116-119`: `NOT_FOUND` → `Google Gemini model not found (HTTP 404): {text}`.
- `shatter-llm/src/anthropic.rs:106-122`: handles 429 and 401 specifically; everything else, including 404, becomes `Anthropic API HTTP {status}: {text}`.
- The CLI path (`shatter-cli/src/helpers.rs:1598-1640`, `build_oracle_adapter`) was not traced to the point where oracle call errors are reported.

## Acceptance criteria

- [ ] Reproduce first, and paste the transcripts into the issue before changing code: for each of anthropic, openai and google, run `shatter explore` with the LLM oracle enabled and a deliberately invalid model name (a mock HTTP server returning each provider's real 404 body is acceptable if no keys are available; state which was used). Record exactly what the user sees on stdout/stderr and in the run's error summary.
- [ ] Every adapter maps a model-rejection response to one shared error variant (e.g. `OracleError::ModelNotFound { provider, model }`) whose message names the model ID and the config key/flag that set it.
- [ ] The CLI surfaces that error once per run as a visible warning (or a hard error if the user explicitly asked for the oracle, whichever the maintainer picks; record the choice), rather than a generic adapter error or silence. Test with a mock server for at least the Anthropic adapter, shown failing on main.
- [ ] `cargo test -p shatter-llm -p shatter-cli` passes.

## Out of scope

- The user guide and the default-model refresh policy (note on str-qwua7.21, slug `qwua7-21-llm-seed-oracle-docs`).
- Changing the default model IDs.

## Size

S

## References

- Finding frontend-rust-18 (audit 2026-09-22). Split from the str-qwua7.21 note after the Codex cross-check.
