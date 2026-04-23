# str-hy9b Expedition Plan

## Goal

Redesign the Go frontend around package-aware discovery, overlay-backed
execution, invocation planning, persistent derived artifacts, and honest
per-target reporting. Replace the current foreign-module direct-call harness
with a credible Go exploration engine.

## Success Criteria

- go/packages-based loader replaces direct foreign-module calls
- Overlay manifest pipeline produces cache-stable wrapper builds
- Invocation planner covers receiver construction, parameter synthesis, and interface stubs
- Honest outcome reporting (per-target pass/fail/error) flows to the protocol layer
- Retirement gate validates old harness is fully removed
- Smoke tests pass; E2E tests pass for the Go path

## Task Sequence

Completed in prior workflow (merged to main):
- str-hy9b.A1: Outcome protocol types
- str-hy9b.B1: Artifact workspace layout
- str-hy9b.C1: Adopt go/packages loader
- str-hy9b.D1: Legal-anchor resolver
- str-hy9b.D2: Overlay manifest generator
- str-hy9b.D5: Concolic instrumentation overlay
- str-hy9b.D7: Internal-method spike fixture
- str-hy9b.H3: Parity matrix update
- str-hy9b.I1: Single-method interface stub

Completed under the expedition (merged into the base branch):
- str-hy9b.A2 Outcome plumbing, A3 Markdown renderer, A4 Empty-report regression
- str-hy9b.B3 Workspace gc
- str-hy9b.C2 Packages-based analyzer, C3 DiscoveredTarget schema, C4 Method classification gate, C5 Constructor catalog
- str-hy9b.D4 Launcher loop harness, D6 Build orchestrator
- str-hy9b.E1 Invocation plan schemas, E2 Target classifier, E3 Receiver planner, E4 Parameter planner primitives, E5 Plan ranking and budget
- str-hy9b.F1 Composite literal synthesis, F2 Runtime values registry, F3 Constructor scoring
- str-hy9b.H4 Conformance tests
- str-hy9b.I2 Multi-method interface stub

Already implemented under prior redesign work (tracked only in shatter-go/CLAUDE.md and parity matrix):
- str-hy9b.G1 adapter_http_nethttp
- str-hy9b.G2 adapter_gin
- str-hy9b.G4 Safety Policy Contract (default gate)
- str-hy9b.H1 Planner parity audit (docs/audits/2026-04-19-go-planner-parity.md)

Remaining (filed in beads; work serially through the expedition):
- str-6juk — str-hy9b.F4: Parameter planner — aggregate types
- str-ptk5 — str-hy9b.F5: Parameter planner — error / chan / func fallbacks
- str-ruw0 — str-hy9b.G3: hint_config_v1 expansion beyond policy.allow
- str-kt63 — str-hy9b.H2: Wire Go planner (invocation_plan) into explorer and orchestrator
- str-ekjh — str-hy9b.H5: Planner-driven E2E test re-enabled (blocked by H2)
- str-sdtu — str-hy9b.I3: Multi-package / embedded interface stubs
- str-mj9r — str-hy9b.J1: Retirement gate — inventory of legacy harness
- str-kzxt — str-hy9b.J2: Retirement gate — remove legacy direct-call harness (blocked by J1)
- str-ga7r — str-hy9b.J3: Retirement gate — CI gate against regression (blocked by J2)
- str-mbmw — str-hy9b.J4: Retirement gate — walkthrough and docs update (blocked by J2)

## Experiment Register

None planned yet.

## Verification Gates

Quick: `npx task test-quick`
Standard (before commit): `npx task test-standard`
Smoke (before closing any issue): `npx task smoke`
E2E (after Go pipeline changes): `cargo test --test e2e_concolic`
Parity (after protocol changes): `npx task parity`
Full (before merge): `npx task check`
