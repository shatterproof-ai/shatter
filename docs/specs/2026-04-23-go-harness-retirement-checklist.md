# Go Legacy Harness Retirement Checklist

- Date: 2026-04-23
- Expedition: str-hy9b (Go frontend redesign)
- Issue: str-mj9r (str-hy9b.J1)
- Downstream: str-kzxt (J2 removal), str-ga7r (J3 CI gate), str-mbmw (J4 walkthrough/docs)

## Scope

"Legacy harness" = the pre-D3/D4 execution path in `shatter-go/`:
the in-tree instrumentation + foreign-module direct-call executor in
`shatter-go/instrument/` plus the cache-stable wrapper generator in
`shatter-go/wrapper/`. It is still the path the protocol handler dispatches
to for every `execute` request (see `shatter-go/protocol/handler.go:625-639`);
the new D4 launcher + D6 builder packages exist but are not wired into
the handler.

Replacement stack:

- **D2 overlay manifest** — `shatter-go/overlay/` (already live; used by launcher)
- **D4 per-target launcher** — `shatter-go/launcher/` (`BuildLauncher`, `Session.Invoke`)
- **D6 build orchestrator + binary registry** — `shatter-go/build/` (`Builder.Build`, `BinaryRegistry`)
- **E-/F-series planner** — `shatter-go/planner/` (receiver + param + runtime-value + composite + fallback)
- **H2 planner wire** — `shatter-core/src/planner_consumer.rs` + Go `handleGetInvocationPlan`

The retirement does **not** remove `shatter-go/harness/runtime.go` (the harness
subprocess JSON loop): the launcher's generated main.go imports it, so it
stays.

This doc is an inventory only. No code is deleted in J1.

## J2 — Code removal (str-kzxt)

### Production code

| File | Purpose | Replacement |
|------|---------|-------------|
| `shatter-go/instrument/executor.go` (2106 LOC) | Legacy one-shot + prepared-harness execution; `ExecuteFunction`, `ExecuteFunctionWithTiming`, `ExecuteWithPreparedHarness`, harness cache dir, scratch dir, build/exec timeout envs | `launcher.BuildLauncher` + `launcher.Session.Invoke` via `build.Builder.Build` + `build.BinaryRegistry` |
| `shatter-go/instrument/instrument.go` | Temp-dir instrumentation entrypoints; `InstrumentFile`, `InstrumentFileWithTiming`, `InstrumentFileToDir` | D2 overlay manifest (`overlay.Builder.AddInstrumented*`) consumed by launcher build |
| `shatter-go/instrument/overlay.go` | Shared `transformFile` supporting both temp-dir and overlay paths | Fold into overlay-only path inside `shatter-go/overlay/` or keep as private AST transform (verify no temp-dir branch remains) |
| `shatter-go/instrument/visitor.go` | AST walker + branch-emit used by both legacy and overlay transforms | Keep — used by overlay transform; audit to confirm no legacy-only code |
| `shatter-go/instrument/symextract.go` | Symbolic-expression extractor | Keep — instrumentation AST utility; shared |
| `shatter-go/instrument/recorder.go` | Runtime branch recorder source template | Keep — referenced by overlay-instrumented files |
| `shatter-go/instrument/entropy.go` | Harness-cache entropy seed | Remove with `executor.go` (only consumer is cache path) |
| `shatter-go/instrument/mcdc.go` | MC/DC helper source template | Keep — referenced by instrumented code; audit for legacy-only callsites |
| `shatter-go/instrument/harness_runtime_source.go` | Emits `harness/runtime.go` source string into legacy temp dir | Remove — launcher imports `shatter-go/harness` as a normal Go dep |
| `shatter-go/instrument/http_harness.go` | Legacy http adapter harness generator | Port to launcher/adapter plumbing (G1 `adapter_http_nethttp` already live as planner + `protocol/nethttp_adapter.go`) |
| `shatter-go/instrument/gin_harness.go` | Legacy gin adapter harness generator | Port (G2 `adapter_gin` already live as `protocol/gin_adapter.go`) |
| `shatter-go/wrapper/wrapper.go` (375 LOC) | Cache-stable `ShatterInvoke` wrapper generator keyed by discovery hash | Launcher generates self-contained main.go; wrapper dispatch folded into launcher |
| `shatter-go/wrapper/overlay.go` | Wrapper-specific overlay entry | Replaced by `overlay.Builder` methods used from `build.Builder` |

### Handler call sites (rewrite, don't delete the file)

| File | Lines | Current behavior | New behavior |
|------|-------|------------------|--------------|
| `shatter-go/protocol/handler.go` | 625 | `instrument.ExecuteWithPreparedHarness` on cached prepare_id | `build.Builder.Build(plan).Invoke(inputs)` via cached `BinaryRegistry` lookup |
| `shatter-go/protocol/handler.go` | 630 | Auto-lookup prepared harness | `BinaryRegistry.Lookup` by discovery hash |
| `shatter-go/protocol/handler.go` | 632 | `ExecuteFunctionWithTiming` one-shot fallback | One-shot `build.Builder.Build` with single `InvocationPlan` |
| `shatter-go/protocol/handler.go` | 637, 639 | Mirror branches | Mirror with launcher/registry |
| `shatter-go/protocol/handler.go` | 366 | `instrument.InstrumentFileWithTiming` for prepare | `overlay.Builder` materialization under workspace |
| `shatter-go/protocol/handler.go` | harness cache maps (`preparedHarnesses`, `lookupPreparedHarness`) | In-process harness objects | Drop; `BinaryRegistry` is the cache of record |
| `shatter-go/protocol/adapter.go` | `instrument.` refs | Adapter dispatch via legacy executor | Route adapter invocation through launcher with adapter-kind in `PlanDescriptor` |
| `shatter-go/protocol/nethttp_adapter.go` | `instrument.` refs | Same | Same |
| `shatter-go/protocol/gin_adapter.go` | `instrument.` refs | Same | Same |
| `shatter-go/setup/loader.go` | `instrument.` refs | Bootstraps legacy cache dirs | Replace with workspace + binaries dir init from `build.Builder` |

### Tests to remove outright

| File | Reason |
|------|--------|
| `shatter-go/instrument/executor_test.go` (62 KB) | Tests `ExecuteFunction*`, `harnessCacheDir`, `SHATTER_HARNESS_CACHE`, scratch-dir fallback, prepared-harness cache — the entire legacy runtime |
| `shatter-go/instrument/instrument_test.go` | Tests `InstrumentFile` temp-dir output |
| `shatter-go/instrument/fuzz_test.go` | Fuzz over `ExecuteFunction` inputs |
| `shatter-go/instrument/property_test.go` | Rapid properties targeting legacy executor |
| `shatter-go/instrument/harness_runtime_source_test.go` | Verifies legacy harness-source emission |
| `shatter-go/wrapper/wrapper_test.go` | Tests `ShatterInvoke` wrapper generation |
| `shatter-go/wrapper/property_test.go` | Rapid properties for wrapper hash stability |

### Tests to rewrite against the launcher

| File | Migration |
|------|-----------|
| `shatter-go/instrument/overlay_test.go` | Drop the "legacy vs overlay parity" halves; keep the overlay transform assertions against `overlay.Builder` |
| `shatter-go/instrument/overlay_e2e_test.go` | Redirect to `launcher.BuildLauncher` end-to-end |
| `shatter-go/instrument/visitor_test.go` | Keep; visitor is shared, but confirm no legacy-specific expectations |
| `shatter-go/instrument/mcdc_test.go` | Keep; MC/DC is overlay-live |
| `shatter-go/instrument/symextract_test.go` | Keep; shared AST util |
| `shatter-go/instrument/entropy_test.go` | Remove with `entropy.go` |
| `shatter-go/instrument/recorder_test.go` | Keep |
| `shatter-go/protocol/handler_test.go` | Rewrite execute-dispatch cases to exercise launcher/registry; keep analyze, protocol framing, outcome mapping |
| `shatter-go/protocol/adapter_test.go` | Rewrite adapter dispatch cases against launcher |
| `shatter-go/protocol/outcome_test.go` | Keep (outcome mapping is legacy-independent) but audit for fixtures that depend on legacy error strings |
| `shatter-go/protocol/property_test.go` | Audit rapid invariants; keep those on protocol framing |

### Fixtures

| Path | Disposition |
|------|-------------|
| `shatter-go/instrument/testdata/overlaypkg/` | Keep; used by `overlay_test.go` overlay assertions |
| `shatter-go/instrument/testdata/rapid/` | Remove with `instrument/property_test.go` |
| `examples/go/internal-method/` | Keep; known-answer fixture for multi-method interface (I2); re-point at launcher once handler swap lands |
| `examples/go/multi-file-service/` | Keep; known-answer fixture for packages-based analyzer (C2); re-point at launcher |
| `shatter-go/harness/testdata/` | Does not exist — no action |

### Env vars + config knobs to drop

| Name | Consumer | Replacement |
|------|----------|-------------|
| `SHATTER_HARNESS_CACHE` | `instrument.harnessCacheDir` | Workspace `binaries/` dir owned by `BinaryRegistry` |
| `SHATTER_HARNESS_SCRATCH` | `instrument` scratch dir fallback | Workspace `runs/<runID>/` owned by `build.Builder` |
| `SHATTER_BUILD_TIMEOUT`, `SHATTER_EXEC_TIMEOUT` (if legacy-only) | `instrument.execTimeout`, `buildTimeout` | Audit — if launcher re-reads them, keep; otherwise remove |

## J3 — CI regression gate (str-ga7r)

Gates that must exist after J2 so legacy cannot regress:

| Gate | What it checks | Where |
|------|----------------|-------|
| `grep -r "instrument\.\(Execute\|Instrument\)" shatter-go/` returns only the `instrument` package itself (or nothing) | No caller of the legacy executor survives | Add to `task parity` or a new `task retirement` |
| `grep -r "wrapper\." shatter-go/ --exclude-dir=wrapper` returns nothing outside launcher/build consumers | Wrapper pkg is unused | Same target |
| `grep -r "SHATTER_HARNESS_CACHE\|SHATTER_HARNESS_SCRATCH" .` returns nothing | Env vars are gone | Same target |
| `go build ./shatter-go/...` succeeds without `shatter-go/instrument/executor.go` | Legacy file is deletable | CI job |
| Handler dispatch path in `protocol/handler.go` contains only `build.Builder.Build` + `BinaryRegistry` (no `instrument.` refs in `execute` branch) | Structural check | New lint or small `go test` in `protocol/` |
| `task e2e` (`cargo test --test e2e_concolic`) passes end-to-end via launcher only | Pipeline works without legacy | Existing E2E, re-run |
| `task smoke` passes | Non-empty Go reports | Existing smoke |
| `task gauntlet` passes (with known exceptions documented in root CLAUDE.md) | Broad CLI surface | Existing gauntlet |
| `task parity` + `task conformance` pass after handler rewrite | Protocol contract intact | Existing parity |

The new retirement gate target should live next to the existing Taskfile
tiers; wire it into `task check` so it runs before merge.

## J4 — Walkthrough and docs (str-mbmw)

Docs that mention or depend on the legacy harness and must be updated
when J2 lands:

| File | Section to revise |
|------|-------------------|
| `shatter-go/CLAUDE.md` | "Prepare Parity Contract"; `SHATTER_HARNESS_CACHE` fallback block (around lines 120-130); execution dispatch description; any reference to `instrument.Execute*` |
| `protocol/parity-matrix.yaml` | Audit `execute` capability row; confirm Go entry references launcher semantics, not legacy |
| `protocol/GOVERNANCE.md` | If it cites legacy executor examples, swap for launcher examples |
| `PLAN.md` | Remove the "legacy foreign-module direct-call" bullet from the retirement roadmap once J2 lands |
| `AGENTS.md` | Audit for legacy harness references |
| `docs/execution-adapters.md` | Replace legacy adapter examples with launcher-dispatched adapter plumbing (G1/G2 already live) |
| `docs/go-frontend-scope-limits.md` | Remove caveats that only applied to legacy executor (if any) |
| `README.md` | Audit for legacy phrasing |
| `docs/specs/2026-04-15-hybrid-fuzzing-design.md` | Audit for legacy pipeline references |

Walkthrough:

| Item | Action |
|------|--------|
| `task walkthrough` output for Go targets | Re-baseline after handler swap; expected differences in timing, "prepare" lines, binary cache hits |
| Walkthrough example set | Confirm `examples/go/internal-method` and `examples/go/multi-file-service` render end-to-end through launcher |
| Walkthrough script(s) — currently no `walkthrough/` dir at repo root; the task is defined in `Taskfile.yml`/`package.json` | Audit the task definition and any referenced script for legacy env vars |
| New walkthrough note | Add a one-line "binary built at `workspace/binaries/`" line if it materially improves the compact demo story; otherwise suppress |

## Ordering constraint

J2 cannot land until the handler is rewritten to dispatch through
`build.Builder` + `BinaryRegistry` + `launcher.Session`. That rewrite
is the core J2 work; the deletions in the J2 table above happen in the
same or a follow-on commit once no caller remains.

## Non-goals for this inventory

- Deleting code (J2).
- Adding the CI gate target (J3).
- Updating walkthrough baseline or docs prose (J4).
- Touching `shatter-go/harness/runtime.go` — launcher depends on it.
- Touching `shatter-go/overlay/`, `shatter-go/planner/`, `shatter-go/launcher/`, `shatter-go/build/` — these are the replacement.

## Verification for J1

- `task test-standard` passes on this branch (inventory-only change).
- No file under `shatter-go/` is modified.
