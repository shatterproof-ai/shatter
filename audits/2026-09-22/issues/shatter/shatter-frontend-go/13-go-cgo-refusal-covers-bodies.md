---
slug: go-cgo-refusal-covers-bodies
kind: new
title: "Go cgo \"detect and refuse\" is signature-only and its only consumer (planner.Classify) is unreachable: refuse every function in a file that imports \"C\", on the live path"
priority: P3
type: bug
labels: [go-frontend, scope-limits, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go cgo "detect and refuse" is signature-only and its only consumer is unreachable: make the refusal real

## Problem

`docs/go-frontend-scope-limits.md` says cgo is a permanent non-goal and that "The Go frontend will detect and refuse cgo-bearing functions at analysis time rather than attempt a partial model", because Z3 cannot reason across the C ABI and analysing around cgo calls would produce unsound tests or misleading coverage.

The implementation does not do that:

- Detection only inspects parameter and result types for the `C` pseudo-package. A function with a Go-typed signature that calls `C.foo()` in its body is not flagged.
- The flag it sets (`HasCGoDep`) is read only by `planner.Classify`, which `deadcode` reports as unreachable, and the Go frontend never emits the `cgo_dependency` unsatisfied-requirement kind in production code. So it is likely that no cgo refusal happens at all, even for signature-level cgo (inferred from code reading, not yet executed).

**Deliverable (chosen):** make the documented behaviour true. The doc's rationale (unsound tests) applies to body calls as much as to signatures, and the doc is the product contract, so narrowing the claim is not the fix here. If the maintainer prefers to narrow the doc instead, that is a different issue and this one should be closed as won't-fix with that decision recorded.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `docs/go-frontend-scope-limits.md:36-40` (cgo section): the "detect and refuse" claim.
- `shatter-go/protocol/discovered_target.go:100-125` `fnHasCGoDep` checks only `fn.Type.Params` and `fn.Type.Results`; it sets `HasCGoDep` at `:201`.
- `HasCGoDep` has one non-test Go reader: `shatter-go/planner/classify.go:54` inside `Classify` (`:42`). `cd shatter-go && deadcode ./...` lists `planner/classify.go:42:6: unreachable func: Classify`.
- `HasCGoDep` is also a wire field: `shatter-go/protocol/types.go:59` `HasCGoDep bool json:"has_cgo_dep,omitempty"`. Nothing in `shatter-core` reads `has_cgo_dep` (`grep -rn has_cgo_dep shatter-core/src` is empty).
- `shatter-go/protocol/invocation_plan.go:198` declares `UnsatisfiedRequirementKindCGODependency = "cgo_dependency"`; no non-test code emits it. The core accepts it (`shatter-core/src/protocol.rs:313`) and `protocol/parity-matrix.yaml:756` lists it among `UnsatisfiedRequirementKind` variants the Go frontend emits.
- Audit finding frontend-go-13 (code reading; body-call case not reproduced). str-hy9b.H5 (closed) wrote the scope-limits doc.

## Acceptance criteria

- [ ] **Red state recorded first.** A probe fixture under `shatter-go` testdata with (i) a function whose parameter type is `C.int` and (ii) a Go-typed function whose body calls `C.puts`, run through `shatter analyze` and `shatter explore` on main; paste both outcomes into the issue.
- [ ] **Detection.** Every function in a file that imports `"C"` is flagged (the conservative file-level rule); the signature check stays as a subset. Method and closure bodies are covered by the file-level rule.
- [ ] **Refusal on the live path.** Flagged targets reach the output as skipped/unsupported with unsatisfied-requirement kind `cgo_dependency` and a reason naming cgo, from the production analyze/explore/scan paths (not from the dead `planner.Classify`). Neither target in the probe is executed.
- [ ] Tests for both probe shapes, in shatter-go and at the CLI level (explore and scan), each failing on main and passing on the branch.
- [ ] **`has_cgo_dep` wire field:** either given a consumer in shatter-core (e.g. the scan report's skip reason) or removed from `protocol/types.go` and the protocol schema/registry; the choice is recorded in `shatter-go/CLAUDE.md`. If removed, regenerate bindings from the schema rather than editing them.
- [ ] **`planner.Classify`:** if the refusal does not reuse it, comment on the issue filed from `go-dead-code-and-property-targets` that `Classify` can be deleted (it is on that issue's allowlist until then). If it is reused, it is wired into the production path and removed from that allowlist.
- [ ] `protocol/parity-matrix.yaml:756` remains true (Go now emits `cgo_dependency`); `task parity` and `task conformance` pass; `task affected` passes with `Gates selected` recorded.

## Suggested approach

Put the refusal where other unsupported-target reasons are produced on the live path. The file-level "imports C" rule needs only the parsed file's imports, so it can sit next to `fnHasCGoDep` in `discovered_target.go`.

## Out of scope

- Any cgo support or modelling.
- Narrowing the doc claim (a maintainer decision; see Problem).

## Dependencies

- Blocked by: none.
- Related: str-hy9b.H5 (closed; wrote the doc), `go-dead-code-and-property-targets` (holds `planner.Classify` on its allowlist until this decides).

## Size

S-M

## References

- Finding frontend-go-13 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-11). The unreachable-consumer observation was added while re-verifying for this draft. Revised after cross-check: deliverable fixed to "refuse" (Codex finding 12), wire field `has_cgo_dep` addressed (same-runtime review).
