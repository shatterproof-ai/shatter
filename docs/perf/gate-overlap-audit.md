# Gate Overlap Audit (str-35vtk.3)

Audit of suspected redundancy between gates, per the efficiency-plan rule:
remove only provably identical coverage.

## `core:test-ignored` ⊇ `task e2e` — VERIFIED

`core:test-ignored` runs `cargo test -p shatter-core -- --include-ignored`
(shatter-core/Taskfile.yml), which builds and runs **every** test target of
shatter-core — unit tests plus all integration-test binaries under
`shatter-core/tests/`, including `e2e_concolic.rs`, `e2e_concolic_rust.rs`,
and `e2e_concolic_go.rs`, with `#[ignore]`d tests included.

The `e2e` task runs exactly those three binaries (`e2e_concolic` without
`--include-ignored`; the other two with it). Therefore any `check` run (whose
integration stage includes `core:test-ignored`) executes a strict superset of
the `e2e` task's tests. This is the coverage guarantee that lets per-issue
gates (WS-E, str-35vtk.8) trigger e2e selectively: e2e always re-runs at the
landing gate regardless of per-issue selection.

Caveat: `core:test-ignored` does not set `SHATTER_GO_FRONTEND_BIN`, so inside
it the Go-frontend e2e tests fall back to building the Go frontend at test
time (one `go build`, warm GOCACHE). Coverage is identical; only binary
provenance differs.

## `parity` duplication in Taskfile.yml — FIXED (was pure waste)

`parity` was defined twice; go-task silently uses the last definition, so the
first (validate-parity.py only) was dead text. `check` also both dep'd on
`parity` and re-ran it as a cmd — the same full parity suite (registry
validation, codegen check, validate-parity, golden tests) executed twice per
check. Fixed: one definition, one execution (in `check-integration`).

## `conformance` vs `parity` golden tests — NOT deduplicated (not identical)

`parity` ends by running `protocol/conformance/run_golden_tests.py` over
`golden_cases.yaml` (5 cases: recorded-output comparison across frontends).
`conformance` runs `conformance_harness.py` over `conformance_cases.yaml`
(18 cases: live protocol-semantics validation). Only two case *names* overlap
(`handshake`, `shutdown`), and the two harnesses assert different things
(golden: byte-level agreement with recorded expectations; conformance:
behavioral contract of a live session). Not provably identical — both stay.
