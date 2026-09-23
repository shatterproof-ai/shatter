---
slug: fuzz-policy-vs-reality
kind: new
title: "Fuzzing policy vs reality drift"
priority: P3
type: task
labels: [testing, fuzzing, docs, formal-methods, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Fuzzing policy vs reality drift

## Problem

The `formal-methods-policy` skill, which agents load when they decide how to test new code, and the matching section of `shatter-core/CLAUDE.md` both tell agents that Rust deserialization boundaries use `cargo-fuzz` and that Go uses native `testing.F` fuzzing in `*_fuzz_test.go` files. The repo does neither in the sense the policy implies:

- There is no cargo-fuzz crate. Rust "fuzzing" is a proptest suite that feeds random byte vectors to serde entry points (`shatter-core/tests/fuzz_deserialization.rs`), with no coverage guidance.
- There are 22 Go `Fuzz*` targets, but no Taskfile task, script or workflow runs `go test -fuzz=`. Plain `go test` executes only their `f.Add` seed corpus, so they are regression tests, not fuzzers. There is no `testdata/fuzz/` corpus directory either.
- The files are named `fuzz_test.go`, not `*_fuzz_test.go` as the policy says.

Agents that follow the policy will either add a cargo-fuzz target that no gate runs, or believe coverage exists that does not. The policy has no drift check against the code.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `.claude/skills/formal-methods-policy/SKILL.md:14`: "**Native fuzzing** (Go `testing.F`, `cargo-fuzz`) | Crash resistance at parsing boundaries". `:34-38`: "## Native Fuzzing ... **Go**: `testing.F` in `*_fuzz_test.go` ... **Rust**: `cargo-fuzz` for deserialization boundaries." `:63`: "`*_fuzz_test.go` for byte-level fuzzing".
- `shatter-core/CLAUDE.md:28,50-54` and `shatter-go/CLAUDE.md:34` say the same.
- `ls fuzz shatter-core/fuzz` returns "No such file or directory". `git ls-files | grep -i fuzz` lists only `shatter-core/tests/fuzz_deserialization.rs`, `shatter-core/src/fuzzer.rs` (the engine's input fuzzer, unrelated), `shatter-go/{instrument,protocol}/fuzz_test.go`, a protocol stub script and two 2026-04-15 hybrid-fuzzing plan/spec docs.
- `shatter-core/tests/fuzz_deserialization.rs:7-8`: "These use proptest (not cargo-fuzz) so they run in CI without nightly. For deeper coverage-guided fuzzing, consider adding cargo-fuzz targets later." Case count comes from `SHATTER_FUZZ_CASES` (`Taskfile.yml:144,192` set 32 for the fast tiers; `check` sets 1000 at :495).
- Go targets: `grep -c '^func Fuzz'` gives 8 in `shatter-go/instrument/fuzz_test.go` and 14 in `shatter-go/protocol/fuzz_test.go`. The audit area note said 10; the current count is 22.
- `grep -rln -- '-fuzz=\|-fuzztime\|cargo fuzz\|cargo-fuzz' Taskfile.yml */Taskfile.yml .github scripts` returns no matches.
- Tracker history:
  - **str-df9g** (closed 2026-03-07, reason "Closed") asked for "cargo-fuzz **or proptest bytes-based fuzzing**" for Request/SymExpr/TypeInfo/YAML spec parsing. 218e59c6 (2026-03-06, "test(core): add proptest fuzz targets for deserialization boundaries") delivered the proptest option. So str-df9g was **closed fixed under its own either/or acceptance**, not closed-unfixed. The policy text is what drifted.
  - **str-l02k** (closed 2026-03-06) and **str-aslo** (closed 2026-03-30) added the Go native fuzz targets. Neither added a job that runs them with `-fuzz`.
- Audit finding tests-ci-14 (verified, P3). Area evidence: `audits/2026-09-22/areas/tests-ci.md` T-14.

## Acceptance criteria

The maintainer picks exactly one of the three policies below, and the choice is recorded as an issue comment before implementation. Each option is internally consistent: what the policy says is run is exactly what a gate or scheduled job runs.

**Option A: coverage-guided fuzzing for Go and Rust**
- [ ] A scheduled (weekly) workflow, or a drift-patrol step, runs each Go `Fuzz*` target with a bounded `-fuzztime` (e.g. 60 s per target), and a `cargo-fuzz` crate covering at least the protocol `Request`/`Response` and `SymExpr`/`TypeInfo` deserializers (nightly toolchain pinned for that job only).
- [ ] New crashers are committed as `testdata/fuzz/<Target>/` (Go) or corpus/regression files (Rust), so they become regression seeds.
- [ ] The policy docs say Go and Rust coverage-guided fuzzing run on that schedule, and name the job.

**Option B: coverage-guided fuzzing for Go only**
- [ ] A scheduled (weekly) workflow, or a drift-patrol step, runs each Go `Fuzz*` target with a bounded `-fuzztime`; crashers are committed as `testdata/fuzz/<Target>/` seeds.
- [ ] The policy docs say: Go uses `testing.F` targets, run as seed-corpus regression tests in `go test` and as coverage-guided fuzzers in the named scheduled job; Rust byte-level fuzzing is proptest in `tests/fuzz_deserialization.rs` driven by `SHATTER_FUZZ_CASES`, and `cargo-fuzz` is explicitly not used.

**Option C: no coverage-guided fuzzing**
- [ ] The policy docs say: Go `testing.F` targets run only as seed-corpus regression tests in `go test`; Rust byte-level fuzzing is proptest in `tests/fuzz_deserialization.rs`; coverage-guided fuzzing (`go test -fuzz`, `cargo-fuzz`) is explicitly not run.

**For every option**
- [ ] `.claude/skills/formal-methods-policy/SKILL.md` (table at :14, "Native Fuzzing" at :34-38, :63), `shatter-core/CLAUDE.md` (:28, :50-54) and `shatter-go/CLAUDE.md` (:34) state the chosen policy and nothing contradicting it.
- [ ] The `*_fuzz_test.go` naming claim is corrected to `fuzz_test.go`, or the files are renamed to match.
- [ ] A drift check (e.g. in `scripts/drift-patrol.py`) validates affirmative execution claims: for each fuzz mechanism the policy says is run (`-fuzz`/`-fuzztime`, `cargo fuzz`), it fails unless some Task, script or workflow invokes it. Mechanisms the policy explicitly describes as not used are allowed to be named. The check has a unit test with a fixture policy that claims an un-invoked mechanism (fails) and one that names it as not used (passes).
- [ ] For options A and B, proof at close: the URL of a green scheduled or `workflow_dispatch` run showing each target's fuzz duration. For option C, proof at close: the drift check output on the final branch.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Option B is likely the best cost/benefit: it turns the 22 existing Go targets into real fuzzers with a small weekly job and avoids a nightly Rust toolchain. Choose A only if coverage-guided Rust fuzzing is worth a nightly toolchain in one scheduled job.

## Out of scope

- The engine's own input fuzzer (`shatter-core/src/fuzzer.rs`) and the hybrid-fuzzing design docs. Those are product features, not test policy.
- proptest/fast-check/rapid property-test coverage policy beyond the fuzzing rows.

## Priority / type / labels

P3 · task · testing, fuzzing, docs, formal-methods, audit · Size S (C) / M (A, B)

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-df9g, str-l02k, str-aslo (all closed).
