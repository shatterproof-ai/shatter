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

Remaining (from beads tracker — see below for open items):
- Group A: Outcome plumbing (A2, A3, A4, A5)
- Group B: Workspace gc (B2, B3)
- Group C: Packages-based analyzer + DiscoveredTarget + classifiers (C2–C6)
- Group D: Build orchestrator and derived artifacts (D3, D4, D6)
- Group E: Invocation plan schemas and planners (E1–E5)
- Group F: Argument synthesis (F1–F5)
- Group G: Side-effect safety and hints (G1–G4)
- Group H: Explorer planner audit and conformance (H1, H2, H4, H5)
- Group I: Multi-method interface stubs (I2, I3)
- Group J: Retirement gate (J1–J4)

## Experiment Register

None planned yet.

## Verification Gates

Quick: `npx task test-quick`
Standard (before commit): `npx task test-standard`
Smoke (before closing any issue): `npx task smoke`
E2E (after Go pipeline changes): `cargo test --test e2e_concolic`
Parity (after protocol changes): `npx task parity`
Full (before merge): `npx task check`
