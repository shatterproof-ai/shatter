---
slug: fuzz-policy-vs-reality
kind: new
title: "formal-methods-policy prescribes cargo-fuzz and Go native fuzzing; reality is proptest byte-fuzz and seed-corpus-only Go Fuzz targets that nothing mutates"
priority: P3
type: task
labels: [testing, fuzzing, docs, formal-methods, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# formal-methods-policy prescribes cargo-fuzz and Go native fuzzing; reality is proptest byte-fuzz and seed-corpus-only Go Fuzz targets that nothing mutates

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

The maintainer picks option A or B, and the choice is recorded in the issue before implementation.

**Option A: make reality match the policy**
- [ ] A scheduled (weekly) workflow, or a drift-patrol step, runs each Go `Fuzz*` target with a bounded `-fuzztime` (e.g. 60 s per target). New crashers are committed as `testdata/fuzz/<Target>/` seed files, so they become regression seeds.
- [ ] Either a `cargo-fuzz` crate covering at least the protocol `Request`/`Response` and `SymExpr`/`TypeInfo` deserializers runs in the same scheduled job (nightly toolchain pinned for that job only), or the policy states that proptest byte-fuzzing is the Rust standard (see B).
- [ ] Proof at close: the URL of a green scheduled or `workflow_dispatch` run showing each target's fuzz duration.

**Option B: make the policy match reality**
- [ ] The SKILL.md table and the "Native Fuzzing" section, `shatter-core/CLAUDE.md` and `shatter-go/CLAUDE.md` describe what exists: Go `testing.F` targets in `fuzz_test.go` run as seed-corpus regression tests in `go test`, and Rust byte-level fuzzing is proptest in `tests/fuzz_deserialization.rs` driven by `SHATTER_FUZZ_CASES`. Coverage-guided fuzzing is named as not currently run.
- [ ] The `*_fuzz_test.go` naming claim is corrected to `fuzz_test.go`, or the files are renamed to match.

**Either option**
- [ ] A cheap drift check (for example in `scripts/drift-patrol.py`) fails when the policy names a fuzz mechanism (cargo-fuzz, `-fuzz`) that no Task or workflow invokes.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Option B plus a bounded Go `-fuzztime` step in the existing weekly drift-patrol workflow costs the least, and turns the 22 existing Go targets into real fuzzers. Add cargo-fuzz only if the maintainer wants coverage-guided Rust fuzzing enough to accept a nightly toolchain in one scheduled job.

## Out of scope

- The engine's own input fuzzer (`shatter-core/src/fuzzer.rs`) and the hybrid-fuzzing design docs. Those are product features, not test policy.
- proptest/fast-check/rapid property-test coverage policy beyond the fuzzing rows.

## Priority / type / labels

P3 · task · testing, fuzzing, docs, formal-methods, audit · Size S (B) / M (A)

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-df9g, str-l02k, str-aslo (all closed).
