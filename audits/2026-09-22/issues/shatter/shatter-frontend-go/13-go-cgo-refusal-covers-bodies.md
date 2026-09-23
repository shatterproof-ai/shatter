---
slug: go-cgo-refusal-covers-bodies
kind: new
title: "Go cgo \"detect and refuse\" is signature-only, and its only consumer (planner.Classify) is unreachable: verify, then refuse body C.* calls or narrow the claim"
priority: P3
type: bug
labels: [go-frontend, docs, scope-limits, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go cgo "detect and refuse" is signature-only, and its only consumer (planner.Classify) is unreachable: verify, then refuse body C.* calls or narrow the claim

## Problem

`docs/go-frontend-scope-limits.md` says cgo is a permanent non-goal and that "The Go frontend will detect and refuse cgo-bearing functions at analysis time rather than attempt a partial model", because Z3 cannot reason across the C ABI and analysing around cgo calls would produce unsound tests or misleading coverage.

The detection only inspects parameter and result types for the `C` pseudo-package. A function with a Go-typed signature that calls `C.foo()` in its body is not flagged. Re-verification for this draft found a second gap: the flag it sets (`HasCGoDep`) is read only by `planner.Classify`, which `deadcode` reports as unreachable, and the Go frontend never emits the `cgo_dependency` unsatisfied-requirement kind in production code. So it is possible that no cgo refusal happens at all, even for signature-level cgo. That is inferred from code reading and has not been checked by execution.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `docs/go-frontend-scope-limits.md:36-40` (cgo section): the "detect and refuse" claim.
- `shatter-go/protocol/discovered_target.go:100-125` `fnHasCGoDep` checks only `fn.Type.Params` and `fn.Type.Results`; it sets `HasCGoDep` at `:201`.
- `HasCGoDep` has one non-test reader: `shatter-go/planner/classify.go:54` inside `Classify` (`:42`). `cd shatter-go && deadcode ./...` lists `planner/classify.go:42:6: unreachable func: Classify`.
- `shatter-go/protocol/invocation_plan.go:198` declares `UnsatisfiedRequirementKindCGODependency = "cgo_dependency"`; no non-test code emits it. The core accepts it (`shatter-core/src/protocol.rs:313`) and `protocol/parity-matrix.yaml:756` lists it among `UnsatisfiedRequirementKind` variants the Go frontend emits.
- Audit finding frontend-go-13 (code reading; body-call case not reproduced). str-hy9b.H5 (closed) wrote the scope-limits doc.

## Acceptance criteria

- [ ] First, record the actual behaviour: a probe file with (i) a function whose parameter type is `C.int` and (ii) a Go-typed function whose body calls `C.puts`, run through `shatter analyze` and `shatter explore`; paste the outcomes in the issue.
- [ ] Then either:
  - (a) the Go frontend refuses both shapes: body selectors resolving to the `"C"` package are detected (via `types.Info.Uses` → `*types.PkgName` with path `"C"`, or by flagging every function in a file that imports `"C"`), and the refusal actually reaches the output as `cgo_dependency` (skipped/unsupported with that reason), with tests for both shapes that fail before and pass after; or
  - (b) the claim is narrowed: `docs/go-frontend-scope-limits.md`, `shatter-go/CLAUDE.md` and the `parity-matrix.yaml` notes state exactly what is detected and what happens to undetected cgo functions.
- [ ] If (a) re-uses `planner.Classify`, it is wired into the production path (and removed from the deadcode allowlist in `go-dead-code-and-property-targets`); if not, that issue is told `Classify` can be deleted.
- [ ] `task parity` passes if the matrix changes; `task affected` passes with `Gates selected` recorded.

## Suggested approach

(a) is preferred because the doc's rationale (unsound tests) applies to body calls as much as to signatures. The file-level "imports C" rule is simplest and conservative. Put the refusal where other unsupported-target reasons are produced in the live path, not in the dead planner code, unless the planner is being revived for other reasons.

## Out of scope

- Any cgo support or modelling.

## Dependencies

- Blocked by: none.
- Related: str-hy9b.H5 (closed; wrote the doc), `go-dead-code-and-property-targets` (holds `planner.Classify` on its allowlist until this decides).

## Size

S

## References

- Finding frontend-go-13 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-11). The unreachable-consumer observation was added while re-verifying for this draft. No earlier draft.
