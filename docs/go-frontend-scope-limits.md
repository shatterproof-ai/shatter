# Go Frontend Scope Limits

This document records what the Go frontend does and does not analyze, and — for things it does not — whether that is a **permanent non-goal** or a **deferred capability** tracked for later.

Anything on this list that is *deferred* has a beads issue; anything classified *permanent* will not get one, because there is no intent to implement it.

Companion docs:
- `protocol/parity-matrix.yaml` — authoritative cross-frontend capability matrix (issues get rows here when they are promoted from "deferred" to "in flight").
- `shatter-go/CLAUDE.md` — per-capability parity contracts (side effects, invocation model, ite SymExpr, loop snapshots, prepare, timeouts).

## `vendor/` directories

**Implemented.** Closed: `str-nm5e`.

Modules that vendor their dependencies (`go mod vendor` producing `vendor/modules.txt` alongside the vendored tree) are resolved through `vendor/` automatically. The analyzer's `go/packages` loader inherits the toolchain's vendor-mode auto-detection (Go ≥ 1.14: a `vendor/modules.txt` consistent with `go.mod` enables `-mod=vendor` by default), so vendored imports flow through `pkg.TypesInfo` exactly like module-cache imports. Regression coverage lives in `shatter-go/protocol/vendor_test.go`, which builds an isolated module with `GOPROXY=off` to prove vendor is the only resolution path. Pre-1.14 GOPATH-style `vendor/` trees (no `modules.txt`) are not exercised; modern `go mod vendor` output is the supported shape.

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

**Implemented.** Generated files are skipped by default (str-1giz).

Generated files (protoc-gen outputs, `stringer`, `mockgen`, controller-gen, etc.) are routinely overwritten on the next regeneration, so any harness produced from them is throw-away work. Analyzing them also wastes cycles on machine-emitted boilerplate that rarely encodes interesting domain logic. The Go frontend therefore rejects generated files at every command entry point (`analyze`, `instrument`, `prepare`, `execute`) with `not_supported`, mirroring the existing `_test.go` skip.

Two detection mechanisms run independently; either one is sufficient:

1. The standard `// Code generated ... DO NOT EDIT.` header per Go tooling conventions (https://pkg.go.dev/cmd/go#hdr-Generate_Go_files_by_processing_source). The marker must occupy its own `//` comment line and appear before the first non-comment, non-blank line of the file.
2. Well-known generated-code filename patterns: `*.pb.go`, `*.pb.gw.go`, `*_gen.go`, `zz_generated*.go`.

There is currently no override flag — if a project genuinely needs analysis of a header-marked or pattern-matching file, rename it or strip the marker. An opt-in override can be added if real workloads call for it.

## How to extend

When a deferred item is promoted from this doc to active implementation:

1. Update the beads issue to the appropriate state and assign it.
2. Add or update the relevant row in `protocol/parity-matrix.yaml` (and, if it is a frontend-asymmetric capability, add an entry to `allowed_divergences` while it is partial).
3. Move the item from this doc into the appropriate parity-contract section of `shatter-go/CLAUDE.md`.
4. Once the capability is complete and parity is achieved across frontends, remove the item from this doc entirely.

Keep permanent non-goals in this doc indefinitely — they function as a pre-answered FAQ for contributors and for agents planning new work.
