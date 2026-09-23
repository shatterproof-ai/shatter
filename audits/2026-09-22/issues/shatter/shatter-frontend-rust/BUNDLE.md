# Audit 2026-09-22 — bucket `shatter-frontend-rust` (repo: shatter)

Theme: Rust frontend, runtime and shatter-llm: crate-bridge stdout, constraint encoding, timeout budgets, panic boundary, usize inputs, duplicated tests.

Tracker: bd in /home/ketan/project/shatter (prefix `str`). Parent epic for every item: "Epic: Audit 2026-09-22 findings". Nothing in this bundle has been filed. Evidence was re-verified against the audit worktree at commit 56c86168 where that was cheap; the line numbers are corrected to that commit.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire snapshot `shatter diff` and the unused Snapshot writer path. spec-diff is the regression tool, and SPEC/README/QUICKSTART are updated to match. The `diff` name becomes free; whether str-81xiw takes it is left to that epic. The shatter-agents docs for `shatter diff --staged` are corrected.
- **D3 Concolic positioning:** measure first. P1 benchmark comparing default and concolic, and P1 fix for concolic early termination. A later decision issue re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and move tracker sync to a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering the DB. AGENTS.md drops `bd sync`, str-qwua7.28 is superseded, and bento gets matching guidance. No hook-timeout env var or bypass guidance.
- **D5 Git identity:** the leaked `[user]` section has already been removed. Add `.mailmap`, a git-state check (fold into str-qwua7.1), and a before/after `.git/config` snapshot in the fixture-isolation test.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

None of D1-D6 changes an item in this bucket.

## Contents

| # | Slug | Kind | Target | Priority | Blocked by |
|---|---|---|---|---|---|
| 01 | rust-crate-bridge-stdout | new | - | P1 | - |
| 02 | rust-instrument-constraints | new | - (approach via str-qwua7.36) | P2 | - |
| 03 | timeout-budget-invariant | new | - | P2 | - |
| 04 | qwua7-50-panic-boundary | note-to-existing | str-qwua7.50 | P2 | - |
| 05 | rust-usize-negative-inputs | new | - | P2 | - |
| 06 | rust-usize-reopen-note | reopen-note | str-ddxe | P2 | rust-usize-negative-inputs (filing order only) |
| 07 | rust-main-duplicate-module-tree | new | - | P2 | - |
| 08 | rust-tests-offline-silent-pass | new | - | P2 | - |
| 09 | rust-frontend-design-dedupe | new | - | P3 | - |
| 10 | rust-runtime-harness-loop | new | - | P3 | - |
| 11 | shatter-llm-hardening | new | - | P3 | - |
| 12 | qwua7-21-llm-seed-oracle-docs | note-to-existing | str-qwua7.21 | P3 | - |


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

The crate-bridge driver writes its JSON result line with `println!` to the same stdout the target function writes to. It redirects no file descriptors. The reader in `PersistentHarness::execute` treats the first stdout line as the protocol response. So when a target function calls `println!`, the protocol line is corrupted, and the execution is reported as `internal_error` / `runtime_failed` instead of returning the function's value.

Crate-bridge is not an opt-in mode. It is the automatic fallback whenever the bin-only dispatch path returns `NonExecutable` for a file inside a crate, so real crates hit it without asking. The standalone and dispatch harnesses already redirect fd 1/fd 2 with `dup2`. Only crate-bridge does not.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/executor.rs:5195`: the generated crate-bridge driver emits `println!("{}", serde_json::to_string(&exec_result).unwrap());`, and nothing around it redirects fds.
- `dup2` appears only in the standalone generator (`executor.rs:2541-2561`) and the dispatch generator (`executor.rs:2720-2881`).
- `executor.rs:627-647` `PersistentHarness::execute` reads one line from `response_rx` and runs `serde_json::from_str(&line)`. On failure it returns `OutputParseError("failed to parse execute result: ...\nline: {line}")`.
- `executor.rs:6292-6300`: crate-backed files go to bin-only and fall back to crate_bridge unless the caller requested a harness mode explicitly.
- `protocol/parity-matrix.yaml:492,496` and `shatter-rust/CLAUDE.md:21,23` record only that "crate-bridge does not capture console output". They do not record that user output corrupts the protocol channel.
- Audit probe (finding frontend-rust-01): a crate containing `pub fn noisy(n: i64) -> i64 { println!("hello from user code {n}"); n + 1 }`, executed with `harness_mode: crate_bridge`, returned `error internal_error output parse error: failed to parse execute result: expected value at line 1 column 1 line: hello from user code 1`, with outcome `runtime_failed`. A sibling non-printing function worked after the harness restarted.

## Acceptance criteria

- [ ] The crate-bridge protocol uses a channel user code cannot write to. Either (a) dup the original stdout to a private fd at driver startup and point fd 1 (and fd 2 if needed) at a capture file or `/dev/null` around each call, or (b) prefix protocol lines with a sentinel and have the reader skip lines without it.
- [ ] Regression test, first failing and then passing on the branch: a crate-bridge-routed crate with a printing function returns its value (e.g. `noisy(1) == 2`) with no `internal_error`. Record the failing run's output in the close note.
- [ ] Console output from crate-bridge is either captured as `console_output` or deliberately discarded. Whichever is chosen, `protocol/parity-matrix.yaml` and the side-effect contract in `shatter-rust/CLAUDE.md` state it, and `task parity` plus `task conformance` pass.
- [ ] `cargo test --test e2e_concolic_rust` passes. Add an E2E known-answer case with a printing target in a crate if none of the existing cases routes through crate-bridge.

## Suggested approach

Option (a) matches what the standalone and dispatch harnesses already do. The finding notes that crate-bridge cannot add a `libc` dependency to the user crate, so use `std::os::fd` (`OwnedFd`, `File::from`) with a small `unsafe` `dup`/`dup2` shim via `extern "C"`, or reopen `/dev/stdout` into a private handle before the first call. Option (b) is simpler but still interleaves user output with protocol output on one pipe, and a user line that happens to start with the sentinel would still break it.

## Out of scope

- Capturing other side-effect kinds in crate-bridge.
- Changing the bin-only / crate-bridge routing policy.

## Size

M

## References

- Finding frontend-rust-01 (audit 2026-09-22, `audits/2026-09-22/findings.json`). Old draft: `drafts/shatter-code/58-rust-crate-bridge-stdout.md`.
- Related closed crate-bridge work (not duplicates): str-qsrb, str-70m2.

---

<!-- file: 02-rust-instrument-constraints.md -->

---
slug: rust-instrument-constraints
kind: new
title: "Rust instrumentation emits invalid or stringly-typed branch constraints: hand-rolled JSON breaks on `1.` floats and control chars; match arms become string equality on pattern text"
priority: P2
type: bug
labels: [rust-frontend, instrumentation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust instrumentation emits invalid or stringly-typed branch constraints: hand-rolled JSON breaks on `1.` floats and control chars; match arms become string equality on pattern text

## Problem

shatter-rust's instrumentor builds runtime branch constraints by formatting JSON strings by hand. Two defects follow from that, and Z3 cannot use the constraints in either case.

1. **Invalid JSON.** A float literal written `1.` is emitted as `"value":1.`, which is not valid JSON. Strings containing control characters other than `\n`, `\r`, `\t` (e.g. U+0001) are embedded raw. The runtime cannot parse either constraint and silently records `SymConstraint::Unknown`, with no warning and no counter.
2. **Match arms as string equality.** Every match arm is encoded as `eq(param "<scrutinee token text>", const str "<pattern token text>")`. So for `match n { 0 => .., 1..=5 => .., k if k > 100 => .., _ => .. }` with `n: i64`, the constraints are `n == "0"`, `n == "1 ..= 5"`, `n == "k"` and `n == "_"`. The analyzer's static builder is also wrong for binding arms: it encodes `k if k > 100` as `n == "k"` (a string constant) and ignores the guard.

The core works around part of this for enums by matching raw token text alphanumerically (str-mambd). That workaround does not help integer ranges, bindings, guards or wildcards.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/instrument.rs:671-702` `constraint_for_lit`: float literals use `f.base10_digits()` raw (`:682`). `instrument.rs:704` `escape_json_string` escapes only `\\`, `"`, `\n`, `\r`, `\t`.
- `shatter-rust-runtime/src/lib.rs:178-186`: an unparseable constraint falls back to `SymConstraint::Unknown { hint }` with no warning.
- `shatter-rust/src/instrument.rs:413-446` `visit_expr_match_mut`: builds `{"kind":"bin_op","op":"eq","left":{"kind":"param","name":"<match expr tokens>"},"right":{"kind":"const","type":"str","value":"<pattern tokens>"}}` for every arm, including bindings and wildcards. The param node has no `path` field.
- `shatter-rust/src/analyzer.rs:1750-1771` walks match arms without looking at `arm.guard`. `analyzer.rs:2128-2137` (`build_pattern_sym_expr`, `syn::Pat::Ident`) produces `scrutinee == Const(Str(ident))` for a binding.
- `shatter-rust/CLAUDE.md:81-83` describes the core-side token-text workaround (str-mambd).
- Audit probes (findings frontend-rust-02/03, not re-run): an explore artifact listed `x > 1.` and `s == "a\u{1}b"` constraints as kind `unknown`, and concolic explore never reached `if s == "a\u{1}b"` in 60 iterations. For `clean(n)` (`n + 1 > 5`, then the 4-arm int match above), all 60 concolic iterations used n=0 and reached 3 of 7 returns, while the batch line reported "6 paths, 5/7 branches". The analyzer's typed builder emits `1.0` and `"a\u0001b"` correctly for the same source.

## Acceptance criteria

- [ ] `instrument.rs` builds `protocol::SymExpr` values and serializes them with `serde_json`, with no hand-formatted constraint JSON left. This is the approach of open **str-qwua7.36** (typed SymExpr). Land it with or on top of that issue.
- [ ] Match arms use typed pattern conversion shared with the analyzer. Int literal → `eq(int)`. Range → `and(ge, le)` (respecting `..` vs `..=`). Binding → `true`, or the guard expression with the binding substituted by the scrutinee. Wildcard → negation of the earlier arms. Guards are conjoined on every arm that has one.
- [ ] The analyzer's binding+guard arm encoding is fixed in the same way (no `Const(Str(ident))` for bindings).
- [ ] When `branch_hit` cannot parse a constraint, it emits a warning or increments a counter that shows up in the execute result/telemetry. Silent `Unknown` is gone.
- [ ] Proptest: every constraint string the instrumentor emits parses as a `SymConstraint` that is not `Unknown`, for generated literals (floats incl. `1.`, `1e10`, strings with arbitrary chars) and generated match patterns.
- [ ] E2E known-answer fixture in `shatter-core/tests/e2e_concolic_rust.rs`: an integer match whose arms need Z3 (no literal that constant mining could find), with a range, a guarded binding and a wildcard. Show `cargo test --test e2e_concolic_rust <name>` failing before the change and passing after, and include both outputs in the close note.
- [ ] After the above is green, retire the core token-text workaround described in `shatter-rust/CLAUDE.md:81-83` (or record in the close note why part of it must stay for cross-file `rename_all` enum values), and update that CLAUDE.md section.

## Suggested approach

Do the JSON half through str-qwua7.36: make `instrument.rs` call the analyzer's `build_sym_expr` / `build_pattern_sym_expr` and embed `serde_json::to_string(&expr)` as the `branch_hit` argument. Then extend `build_pattern_sym_expr` for ranges, bindings, guards and wildcards, so the static (analyzer) and runtime (instrument) paths share one encoder. That keeps the parallel-parity rule in the project CLAUDE.md. Post a comment on str-qwua7.36 linking this issue when filing.

## Out of scope

- Other parts of str-qwua7.36's scope (let patterns, arithmetic, bitwise, calls) beyond what the shared encoder needs.
- TS and Go instrumentors.

## Size

M

## References

- Findings frontend-rust-02, frontend-rust-03 (audit 2026-09-22). Old draft: `drafts/shatter-code/59-rust-instrument-constraints.md`.
- Related: str-qwua7.36 (open, typed SymExpr: the approach for the JSON half), str-mambd (closed, enum-domain token-text workaround).

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
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout

## Problem

For a Rust target, the first execute request compiles the harness with cargo and then runs it, all inside one frontend request. The CLI's per-request timeout defaults to 30 s, the same as the CLI-governed build timeout, and shorter than the frontend's own 120 s fallback. So a cold build under load hits the request timeout before the build timeout. The user sees only `request timed out after 30s` with 0 iterations, and nothing points at `--build-timeout` or `--request-timeout`. Closed str-da35 already noted that cold builds needed manual timeout bumps that "should be auto-set". That was never done.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-cli/src/args.rs:561-563`: `--request-timeout`, `default_value_t = 30`. `args.rs:574-577`: `--build-timeout`, `default_value_t = 30`. The same pair recurs at `args.rs:1020/1030`, `1225/1233`, `1339/1347` and `1400/1410` for the other subcommands.
- `PARITY.md:84`: `SHATTER_BUILD_TIMEOUT` / `--build-timeout` governed default `30` s.
- `shatter-rust/src/executor.rs:1017`: `const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 120;` (changed in eeceb7b4). `PARITY.md:96` still says the Rust fallback is 30 s.
- Audit repro under load (finding frontend-rust-04, not re-run): explore on two trivial Rust functions → `concolic observe failed: frontend error: request timed out after 30s` at 32.7 s with 0 iterations. Re-run with `--request-timeout 180 --build-timeout 170` succeeded.

## Acceptance criteria

- [ ] Enforced invariant: for any request that may build (prepare / first execute of a compiled frontend), the effective request timeout is greater than build_timeout + exec_timeout plus a margin. Either derive it when the user has not set `--request-timeout`, or give build-capable requests their own budget. If the user sets values that violate it, warn.
- [ ] Unit test for the invariant covering defaults, user-set build timeout, and user-set request timeout.
- [ ] A request that times out while a build is in progress produces a diagnostic naming `--build-timeout` / `--request-timeout` (and the env vars), not a bare `request timed out after Ns`. Add a test that forces a short request timeout against a slow build.
- [ ] `PARITY.md:96` corrected to the real Rust fallback (120 s), and the `cli_parity_tests` in `shatter-cli/src/helpers.rs` still pass.
- [ ] Proof at close: `cargo test --test e2e_concolic_rust` passes, plus one cold-cache run (fresh `CARGO_TARGET_DIR`) of a Rust explore with default flags that completes. Paste the command and its outcome into the close note.

## Suggested approach

Compute the request timeout in the CLI wiring from the resolved build and exec timeouts, e.g. `max(request_timeout, build_timeout + exec_timeout + 10)` for prepare/first-execute. Leave the 30 s default for requests that cannot build. Check both explorer paths (random `explorer.rs` and concolic `orchestrator.rs`) and every subcommand that takes these flags, per the parallel-parity rule.

## Out of scope

- Reducing cold-build time itself (prefetch, shared target dirs; see str-jyxr).
- Go/TS timeout defaults, beyond keeping the invariant general.

## Size

S

## References

- Finding frontend-rust-04 (audit 2026-09-22). Old draft: `drafts/shatter-code/60-timeout-budget-invariant.md`.
- Related: str-qe9pp (open; conformance `rust/prepare_supported_rust` times out at the 30 s request timeout, a symptom of this), str-da35 (closed; noted that the values should be auto-set), str-jyxr (open; prefetch-timeout symptom).

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
> **Current code (re-verified at 56c86168):**
> - `shatter-rust/src/handler.rs:480` (`Handler::run`) and `:529` (`dispatch`) have no `catch_unwind`, and neither does `shatter-rust/src/main.rs`. The only `catch_unwind` calls are in generated harness code and the generators' native invoke path. A panic while handling a request therefore ends the frontend process (main exits non-zero). No later request in production ever sees a poisoned lock.
> - The cache Mutexes (`executor.rs:406` `CrateHarnessCache`, `:526` `CrateBridgeHarnessCache`, `:733` `HarnessCache`) are only accessed from the dispatch thread. The threads spawned at `executor.rs` ~3055, ~3206, ~5475 only forward child stdout and never touch the caches.
>
> **Correction to the acceptance test as written.** A unit test *can* exercise poison recovery by wrapping a handler call in `catch_unwind` itself and then reusing the handler, so the test is not impossible. But it would pass without fixing anything real, because production cannot survive a panic without a handler-level boundary.
>
> **Proposed re-scope (replaces the poison-tolerant-lock acceptance item):**
> - Add a panic boundary: wrap `dispatch` in `std::panic::catch_unwind(AssertUnwindSafe(..))`, turn a caught panic into an `internal_error` response naming the panic message, and clear or invalidate the harness caches the panicking request may have left inconsistent. Only after that is poison recovery (`lock().unwrap_or_else(PoisonError::into_inner)`) meaningful. **Or** replace the Mutexes with owned `HashMap`s on the handler, since access is single-threaded.
> - Acceptance: an integration test that sends a request that forces a panic inside dispatch and then a normal request **over the stdio protocol to one frontend process**. The second request succeeds and the first gets `internal_error`. Show it failing before the change and passing after.
> - Keep the module-docs half of this issue unchanged.

---

<!-- file: 05-rust-usize-negative-inputs.md -->

---
slug: rust-usize-negative-inputs
kind: new
title: "Rust targets still receive negative integers for usize params (str-ddxe fix incomplete), and the deserialization failures are reported as target throws"
priority: P2
type: bug
labels: [rust-frontend, input-generation, regression, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust targets still receive negative integers for usize params (str-ddxe fix incomplete), and the deserialization failures are reported as target throws

## Problem

Closed str-ddxe added `int_width` / `int_signed` to `TypeInfo::Int`, in-range generation and Z3 range assertions for unsigned Rust ints. Its close reason: "sized int_width/int_signed on TypeInfo::Int; in-range generation + Z3 range assertions; new u8 e2e gate green". Yet exploring a function with a `usize` parameter still produces many negative inputs. The harness rejects them at deserialization, and the report shows them as `throws runtime_error` rows, i.e. as behaviors of the target. The findings are noise twice over: the inputs are invalid, and the rejections are shown as target behavior.

## Evidence

- Audit repro (finding goals-15, reproduced by the verifier with the current release `shatter` + `shatter-rust`): `shatter explore audits/2026-09-22/goals-runs/standalone/rust/18_accept_language.rs:parse_language_preference` (signature `fn parse_language_preference(part: &str, order: usize) -> Option<LanguagePreference>`, line 41) gave 23 rows. 19 of them were `throws runtime_error: input 1 deserialization failed: invalid value: integer `-998`, expected usize`, with values -998, -44, -1, -644, -838 and i64::MIN. Transcript: `audits/2026-09-22/goals-runs/rust-walk.md` §`parse_language_preference`.
- The analyzer maps `usize` correctly (re-verified at 56c86168): `shatter-rust/src/analyzer.rs:791` `"usize" => Some((64, false))`, `analyzer.rs:1290` `"usize" => int_type(64, false)`. So the negative values come from a downstream generation path that ignores `int_signed` (candidates: boundary-value seeds, literal/constant mining, mutation, solver models, seed/corpus replay in `shatter-core/src/input_gen.rs` and the orchestrator). i64::MIN in particular looks like a boundary seed.
- The walkthrough error regex does not match "deserialization failed", so no gate catches these rows (see the prior audit and str-qwua7.14).

## Acceptance criteria

- [ ] Identify which generation path(s) emit negative values for an unsigned `TypeInfo::Int`, and name them in the close note.
- [ ] Unsigned params (`u8`/`u16`/`u32`/`u64`/`u128`/`usize`) never receive negative values, or values above their width, on any path: random, boundary, mutation, literal mining, solver, seed/corpus replay. Add a proptest over `TypeInfo::Int { int_signed: Some(false), int_width }` for every generator/mutator entry point.
- [ ] Harness input-deserialization failures are classified as tool/input errors (not target `throws`) in explore/scan output, matching the direction of str-4yc9w for Go.
- [ ] E2E known-answer test (in `shatter-core/tests/e2e_concolic_rust.rs`, fixture copied into the repo's Rust examples) with a `(&str, usize)` signature asserts zero deserialization-failure rows. Show it failing before the fix and passing after.
- [ ] Post the reopen note on str-ddxe (slug `rust-usize-reopen-note`) linking this issue.

## Suggested approach

Start by diffing the `TypeInfo` emitted for `parse_language_preference` against the u8 E2E fixture str-ddxe used, to confirm `int_signed: Some(false)` reaches the core for `usize`. Then grep `shatter-core/src` for integer candidate producers that construct `Value::Number` from i64 constants without consulting `int_signed` (boundary tables, mined constants, mutation deltas, solver model extraction).

## Out of scope

- Go-side misclassification (str-4yc9w).
- Rust walkthrough param-type disagreement beyond this signature (str-qwua7.14).

## Size

S

## References

- Finding goals-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/82-rust-usize-negative-inputs.md`.
- Related: str-ddxe (closed; the fix this regresses or does not fully cover), str-qwua7.14 (open), str-4yc9w (open, Go counterpart of the misclassification), str-cfsa (closed, earlier Go counterpart).

---

<!-- file: 06-rust-usize-reopen-note.md -->

---
slug: rust-usize-reopen-note
kind: reopen-note
title: "NOTE on closed str-ddxe: usize parameters still receive negative integers"
priority: P2
type: bug
labels: [rust-frontend, input-generation, regression, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [rust-usize-negative-inputs]
existing_id: str-ddxe
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on closed str-ddxe: usize parameters still receive negative integers

Target: **str-ddxe** (closed). Action: post the comment below with `bd comments add str-ddxe ...`. Leave str-ddxe closed. The follow-up work is tracked in the new issue `rust-usize-negative-inputs`, which must be filed first so its id can replace `<rust-usize-negative-inputs id>` (that is the only reason for the blocked_by).

## Comment text

> **Audit 2026-09-22 note** (finding goals-15). The fix recorded here is incomplete on at least one generation path. With the current release `shatter` + `shatter-rust`, `shatter explore 18_accept_language.rs:parse_language_preference` (`fn parse_language_preference(part: &str, order: usize)`) gave 23 rows, 19 of them `input 1 deserialization failed: invalid value: integer `-998`, expected usize` (also -44, -1, -644, -838, i64::MIN). The analyzer still maps `usize` to `int_type(64, false)` (`shatter-rust/src/analyzer.rs:791,1290`), so the negative values come from a downstream generator that ignores `int_signed`. The u8 E2E gate added here does not cover it. Follow-up: **<rust-usize-negative-inputs id>**. It also covers classifying harness deserialization failures as tool errors instead of target throws.

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
- [ ] `task rust-fe:test` and `cargo test --test e2e_concolic_rust` pass.

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
title: "38 shatter-rust tests print 'skipping' and count as passed when cargo cannot reach the network"
priority: P2
type: bug
labels: [rust-frontend, tests, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# 38 shatter-rust tests print 'skipping' and count as passed when cargo cannot reach the network

## Problem

Many shatter-rust executor tests build fixture crates. When the build fails with an offline-looking error, the test prints `skipping ...` to stderr and returns, and nextest records that as a pass. The nextest profile has `status-level = "fail"`, so the skip message is never shown. A CI run or sandbox without network (or with a cold registry cache) can therefore report `rust-fe:test` green while testing none of the crate-building paths.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/executor.rs:7611` `fn is_offline_compile_error_message(msg: &str) -> bool` matches messages such as `spurious network error` and `Could not resolve host`.
- There are 16 references to it in `executor.rs`: the definition plus 15 match guards of the form `Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => { eprintln!("skipping ..."); }` (e.g. around `:8088`, `:8276`). There are 38 `skipping` occurrences in total.
- `.config/nextest-standalone.toml:12` and `:20`: `status-level = "fail"` (with `final-status-level = "slow"` / `"flaky"`), so passing tests' stderr, including the skip line, is not shown.
- Command to see the count: `/usr/bin/grep -c skipping shatter-rust/src/executor.rs` → `38`.

## Acceptance criteria

- [ ] Offline skips are visible as skips, not passes. Either gate the network-dependent tests behind an env var (e.g. `SHATTER_OFFLINE=1` → `#[ignore]`/nextest filterset exclusion), or make the offline branch fail the test when `CI` is set.
- [ ] CI prefetches the fixture crates' dependencies (a `cargo fetch` step, or reusing the prefetch from str-jyxr), so CI never takes the offline path.
- [ ] `rust-fe:test` reports the number of offline skips, and CI asserts it is 0.
- [ ] Proof at close: run `task rust-fe:test` once with networking disabled (e.g. `CARGO_NET_OFFLINE=true` and an empty `CARGO_HOME`), showing the tests are reported as skipped or failed rather than passed, and once normally, showing 0 skips. Paste both outputs into the close note.

## Suggested approach

Replace the 15 inline guards with one helper, e.g. `fn skip_or_fail_offline(msg) -> !`, that panics under `CI` and otherwise prints a marker that the task wrapper counts. Better still, move the network-dependent tests into a nextest group that the offline profile excludes explicitly.

## Out of scope

- General nextest profile changes (see str-ed38.3 and the tests-ci issues in this epic).
- Reducing how many tests build fixture crates.

## Size

S

## References

- Finding frontend-rust-11 (audit 2026-09-22). Old draft: `drafts/shatter-code/63-rust-tests-offline-silent-pass.md`.
- Related: str-jyxr (open, fixture prefetch timeout).

---

<!-- file: 09-rust-frontend-design-dedupe.md -->

---
slug: rust-frontend-design-dedupe
kind: new
title: "shatter-rust: crate type registry rebuilt on every analyze (O(N^2) parses per crate scan), and two independent Axum extractor classifiers"
priority: P3
type: refactor
labels: [rust-frontend, performance, axum, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust: crate type registry rebuilt on every analyze (O(N^2) parses per crate scan), and two independent Axum extractor classifiers

## Problem

Two design inefficiencies in shatter-rust:

1. **Registry rebuilt per analyze.** Every analyze call rebuilds the crate type registry. It walks the crate's `src/` and `syn`-parses every `.rs` file, with no cache on the `Handler`. Scanning N files of one crate costs about N×N file parses (inferred, not measured).
2. **Two Axum extractor classifiers.** `adapters.rs` classifies Axum extractors (12 kinds, keyed by `ParamInfo.type_name`) for the adapter path. `executor.rs` has a separate 5-kind classifier (Path/Query/Json/State/Multipart) that re-parses type strings with `syn` for the generic wrappers. They can disagree, and support added to one is missing from the other.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/analyzer.rs:83` and `:201` both call `build_crate_type_registry(file_path)`, defined at `analyzer.rs:907`.
- `shatter-rust/src/adapters.rs:389` `pub enum AxumExtractorKind`. `shatter-rust/src/executor.rs:1796` `enum AxumExtractor`, used around `executor.rs:2167, 2195, 2222, 2452, 2776, 4989, 6571, 6693` (per the audit; re-check exact sites when implementing).

## Acceptance criteria

- [ ] The crate type registry is cached on the `Handler`, keyed by crate root plus a cheap fingerprint (e.g. max mtime + file count of `src/**/*.rs`), and invalidated on teardown/shutdown or fingerprint change. A test shows a second analyze in the same crate does not re-parse, and that editing a file invalidates the cache.
- [ ] Record `shatter scan` wall time on a multi-file Rust crate (e.g. the pickpackit or an examples crate) before and after in the close note.
- [ ] One extractor classifier in `adapters.rs` returns kind + inner type, and `executor.rs` consumes it (the `executor.rs` enum is deleted). A test asserts classification for every type in `AXUM_EXTRACTOR_TYPES`.
- [ ] Existing Axum tests and `cargo test --test e2e_concolic_rust` pass. The protocol output is unchanged, so no parity-contract update is needed; if output does change, update `protocol/parity-matrix.yaml` and run `task parity` + `task conformance`.

## Suggested approach

Keep the registry cache in a `HashMap<PathBuf, (Fingerprint, Arc<CrateTypeRegistry>)>` on the handler and pass the `Arc` into the analyze functions. For the classifier, extend `AxumExtractorKind` with a method that yields the inner type from a `syn::Type`, and delete the executor copy.

## Out of scope

- Adding support for new extractor kinds (str-la75, str-62pj, str-38in cover individual extractors).
- Cross-crate / multi-file analysis beyond caching (see project memory on single-file analysis).

## Size

M

## References

- Findings frontend-rust-12, frontend-rust-14 (audit 2026-09-22). Old draft: `drafts/shatter-code/64-rust-frontend-design-dedupe.md`.

---

<!-- file: 10-rust-runtime-harness-loop.md -->

---
slug: rust-runtime-harness-loop
kind: new
title: "shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost"
priority: P3
type: bug
labels: [rust-frontend, runtime, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost

## Problem

1. **Malformed requests become empty inputs.** The runtime harness loop parses each request line with `serde_json::from_str(line).unwrap_or_default()`. An unparseable request turns into `Value::Null`, the target runs with empty inputs, and the result is a misleading `input 0 deserialization failed` instead of a protocol error.
2. **Unescaped fallback JSON.** `flush_results`' serialization-error fallback interpolates the error text into a JSON string with `format!` and does not escape it. An error message containing `"` or `\` produces invalid JSON.
3. **Thread-local branch tracking.** Branch/coverage state is `thread_local!`, so branches executed on threads spawned by user code (`std::thread::spawn`, rayon, `tokio::task::spawn_blocking`) are silently dropped. Closed str-dfnu2 / str-oc67 fixed only tokio cross-worker awaits (current-thread runtime). Neither `shatter-rust/CLAUDE.md` nor the parity matrix documents the remaining limitation.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust-runtime/src/lib.rs:558` and `:612`: `let req: Value = serde_json::from_str(line).unwrap_or_default();`, followed by `req["inputs"].as_array().cloned().unwrap_or_default()` (`:559`, `:614`). Similar `unwrap_or_default()` parsing at `:229`, `:256` (args) and `:358` (mocks).
- `lib.rs:340-345` (`flush_results`, defined at `:318`): `format!(r#"{{..."message":"{}"...}}"#, e)`, with no escaping.
- `lib.rs:167`: `thread_local! {` holds STATE.

## Acceptance criteria

- [ ] An unparseable request line (and a request missing `inputs`) produces an explicit protocol-error execute result that names the parse error. The target is not invoked. Unit test for both cases.
- [ ] The `flush_results` fallback is built with `serde_json::json!` (or equivalent). Unit test with an error message containing `"` and `\` asserts the output parses.
- [ ] The args/mocks `unwrap_or_default()` sites either get the same explicit error or carry a comment explaining why a default is correct.
- [ ] Thread limitation: either document it in `shatter-rust/CLAUDE.md` and `protocol/parity-matrix.yaml` (then run `task parity`), or implement a process-global recorder keyed by execution id, with a test where a branch on a `std::thread::spawn` thread is recorded.
- [ ] `cargo test -p shatter-rust-runtime` and `cargo test --test e2e_concolic_rust` pass.

## Suggested approach

Decode into a typed `struct Request { inputs: Vec<Value> }` with `serde_json::from_str::<Request>` and match the error into an `ExecuteResult` with `thrown_error.error_type = "protocol_error"`. Documenting the thread limitation is the cheap first step. A global recorder needs care with concurrent executions and should only be done if a real target needs it.

## Out of scope

- The crate-bridge stdout channel (`rust-crate-bridge-stdout`).
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
title: "shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width/signedness and tries only the first '['; uncapped backoff; no PBT"
priority: P3
type: bug
labels: [llm, security, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width/signedness and tries only the first '['; uncapped backoff; no PBT

## Problem

There are latent defects in the LLM seed-oracle crate (`shatter-llm`). None has a known live trigger today, but each is one call site away from one:

1. **Secret in Debug.** `JevConfig` holds `api_key: String` and derives `Debug`. The containing types (`JevAdapter`, `ReplayDecisionOracle`, `DecisionFrontierRanker`) also derive Debug, and `DecisionFrontierRanker` ends up as `ExploreConfig.frontier_ranker`, with `ExploreConfig` deriving Debug too. A future `{:?}` of the explore config would print the key. No current log site prints it.
2. **Parser type checks.** `type_matches` accepts any integer for `TypeInfo::Int { .. }` regardless of `int_width` / `int_signed` (-1 passes for `u8`, 300 passes for `u8`). `extract_first_json_array` tries only the first `[` in the model output, so prose such as "[note] ... [{...}]" loses the real array.
3. **Backoff.** Retry backoff is `Duration::from_millis(100u64 << attempt)` with no cap. That overflows at attempt 64 (a panic in debug builds) and grows to hours well before that. A server-provided Retry-After is also used uncapped.
4. **Tests.** `parse.rs` parses untrusted model output but has only example tests (about 10) and no property tests.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-llm/src/jev.rs:23` `#[derive(Debug, Clone)]` on `JevConfig`, with `api_key` at `:26`. `jev.rs:47` `#[derive(Debug)]` (JevAdapter). `shatter-llm/src/replay.rs:12` and `shatter-llm/src/decision_ranker.rs:18` `#[derive(Debug)]`.
- `shatter-core/src/orchestrator.rs:102` `#[derive(Debug, Clone)]` on `ExploreConfig`, with `frontier_ranker: Arc<dyn FrontierRanker>` at `:164`.
- `shatter-llm/src/parse.rs:104` `TypeInfo::Int { .. } => v.is_i64() || v.is_u64(),`. `parse.rs:62` `fn extract_first_json_array`.
- `shatter-llm/src/rate_limit.rs:61-62` `retry_after.unwrap_or_else(|| Duration::from_millis(100u64 << attempt))`, bounded only by `max_retries` (`:30`, `:58`). The verifier did not check the configured `max_retries`, so how reachable the overflow/hours case is remains unconfirmed.

## Acceptance criteria

- [ ] API keys in all LLM adapter configs (Jev, anthropic, openai, google, custom) are wrapped in a redacting newtype (e.g. `SecretString` with `Debug` printing `***`) or given a manual `Debug`. A unit test asserts `format!("{:?}", config)` does not contain the key.
- [ ] Integer values are validated against `int_width` / `int_signed` (unsigned rejects negatives, width bounds enforced). Unit tests for u8 -1 / 256 and i8 -129 are rejected.
- [ ] `extract_first_json_array` tries successive `[` candidates until one parses as the expected array shape. Unit test with a leading bracketed prose fragment.
- [ ] Backoff is capped (e.g. `min(100ms · 2^n, 30s)` using `checked_shl`/saturating math), and Retry-After is capped at the same ceiling. Unit test at attempt 63/64 does not panic and returns the cap.
- [ ] Proptest in `parse.rs`: for arbitrary strings and arbitrary `ParamInfo` type lists, `parse_response` never panics and returns only vectors whose values conform to the declared types (including width/sign).
- [ ] `cargo test -p shatter-llm` passes, with the new tests shown failing on the old code where applicable.

## Suggested approach

Add a small `secret.rs` newtype in shatter-llm, reused by `shatter-core/src/config.rs`'s LLM sections if they hold keys too. Reuse core's integer range helper (the one that str-ddxe added for in-range generation) for the width check, rather than re-deriving bounds.

## Out of scope

- User documentation for the LLM oracle and the default-model policy (note on str-qwua7.21, slug `qwua7-21-llm-seed-oracle-docs`).
- The core→shatter-llm dev-dependency cycle (str-qwua7.43).

## Size

S

## References

- Findings frontend-rust-16, frontend-rust-17 (audit 2026-09-22). Old draft: `drafts/shatter-code/66-shatter-llm-hardening.md`.
- Related: str-qwua7.47 (PBT for core modules only), str-dcgk / str-m0ta (closed; built the crate).

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
> - Default models are hard-coded in `shatter-core/src/config.rs:338` (`claude-sonnet-4-6`), `:376` (`gpt-4o`), `:392` (`gemini-2.0-flash`). Nothing records when or how they are refreshed, and nothing warns when a provider rejects a retired model. The adapter list (anthropic/openai/google/custom/local) is documented only in a doc comment at `shatter-cli/src/helpers.rs:1598-1605` (`build_oracle_adapter`).
>
> **Proposed child (acceptance criteria):**
> - `docs/llm-oracle.md` user guide: what the oracle does and when it helps; enabling it (`--llm`, `--llm-adapter`, `--llm-token-budget`, config keys, env vars); **data egress** (what source/context is sent to which provider); cost controls (token budget, replay/cache); local/custom adapters for no-egress use. Linked from README and QUICKSTART.
> - A written default-model refresh policy (who updates the IDs in `config.rs`, when, and how the change is tested), placed next to the defaults or in the guide.
> - `shatter doctor` (or the first oracle call) reports a clear warning when the configured or default model is rejected by the provider, rather than a generic adapter error.
> - Proof at close: guide rendered and linked, and a doctor/explore run with a deliberately invalid model name showing the warning.
