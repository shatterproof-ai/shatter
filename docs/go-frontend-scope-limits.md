# Go Frontend Scope Limits

This document records what the Go frontend does and does not analyze, and — for things it does not — whether that is a **permanent non-goal** or a **deferred capability** tracked for later.

Anything on this list that is *deferred* has a beads issue; anything classified *permanent* will not get one, because there is no intent to implement it.

Companion docs:
- `protocol/parity-matrix.yaml` — authoritative cross-frontend capability matrix (issues get rows here when they are promoted from "deferred" to "in flight").
- `shatter-go/CLAUDE.md` — per-capability parity contracts (side effects, invocation model, ite SymExpr, loop snapshots, prepare, timeouts).

## `vendor/` directories

**Deferred.** Tracked: `str-nm5e` — *Go frontend: vendor/ directory support*.

The analyzer currently resolves imports through the module cache and `GOPATH` only. Projects that vendor their dependencies (`go mod vendor`, or legacy `vendor/` trees) will not have those imports resolved, which can cause missing type information and, in turn, missing invocation-model detection or synthetic-param resolution. A vendored lookup path is additive to the existing resolver and does not require architectural changes.

## `go.work` workspaces

**Deferred.** Tracked: `str-b66s` — *Go frontend: go.work workspace support*.

The analyzer assumes a single-module root. Multi-module workspace mode (`go.work` + multiple `go.mod` files under one checkout) requires walking cross-module dependencies and selecting the active workspace member per file. Until this is implemented, projects built with workspaces should point the analyzer at a single module at a time.

## Build tags

**Deferred.** Tracked: `str-jl9r` — *Go frontend: build-tag-aware analysis*.

Files are parsed with default tags only. Functions or function bodies that are conditional on build tags (e.g., `//go:build linux`, `//go:build integration`) are either invisible or analyzed under the wrong configuration. Fixing this requires threading a tag set through the analyzer, through the harness builder, and through the parity matrix so consumers can see which tag combination was used.

## cgo

**Permanent non-goal.**

Shatter's concolic engine reasons symbolically using Z3, which does not extend across the C ABI. A function that calls into `import "C"` cannot have its C-side effects symbolically tracked, and attempting to analyze around cgo calls would either silently drop constraints (producing unsound tests) or pretend cgo-calling functions are pure (producing misleading coverage claims). The Go frontend will detect and refuse cgo-bearing functions at analysis time rather than attempt a partial model.

## `_test.go` files

**Permanent non-goal.**

Shatter's charter is to *generate* tests for production code. Existing tests are not analysis targets: their purpose is to assert behavior, and re-analyzing them would produce tests-of-tests, which is not a goal of the tool. Skipping `_test.go` is intentional, not an oversight, and will not be revisited.

## Generated code

**Deferred.** Tracked: `str-1giz` — *Go frontend: skip generated files by default*.

Generated files (protoc-gen outputs, `stringer`, `mockgen`, etc.) are currently in-scope by default. Analyzing them wastes cycles and produces tests that get overwritten on the next regeneration. The standard detection heuristic — the `// Code generated ... DO NOT EDIT.` header per Go tooling conventions, optionally combined with filename patterns (`*.pb.go`, `*_gen.go`, `zz_generated_*.go`) — is a straightforward add once we agree on whether the skip is default-on with an override or opt-in.

## How to extend

When a deferred item is promoted from this doc to active implementation:

1. Update the beads issue to the appropriate state and assign it.
2. Add or update the relevant row in `protocol/parity-matrix.yaml` (and, if it is a frontend-asymmetric capability, add an entry to `allowed_divergences` while it is partial).
3. Move the item from this doc into the appropriate parity-contract section of `shatter-go/CLAUDE.md`.
4. Once the capability is complete and parity is achieved across frontends, remove the item from this doc entirely.

Keep permanent non-goals in this doc indefinitely — they function as a pre-answered FAQ for contributors and for agents planning new work.
