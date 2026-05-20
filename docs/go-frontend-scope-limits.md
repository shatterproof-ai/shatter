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

**Implemented.** Closed: `str-b66s`.

Multi-module workspaces (`go.work` listing multiple `use` members, each with its own `go.mod`) are resolved across module boundaries automatically. The analyzer's `go/packages` loader inherits the toolchain's workspace-mode auto-detection: when a `go.work` file exists in the cwd ancestry (or `GOWORK` is set), `go list` resolves cross-module imports by treating the `use` members as one virtual build, so imports across workspace siblings flow through `pkg.TypesInfo` exactly like single-module imports. No analyzer-side flag plumbing is required — passing the full `os.Environ()` and a `Dir` inside the workspace tree to `packages.Config` is sufficient. Regression coverage lives in `shatter-go/protocol/goworkspace_test.go`, which builds an isolated 2-module workspace with `GOPROXY=off` to prove `go.work` is the only resolution path.

## Build tags

**Implemented.** Closed: `str-jl9r`.

Build tags are honored end-to-end via the user's standard Go toolchain env. Setting `GOFLAGS=-tags=tag1,tag2` (or `GOOS`/`GOARCH`) in the environment that launches the Go frontend opts those tags into both layers of the analyzer's tag handling:

1. The **upfront build-tag-exclusion guard** (`build.Context.MatchFile` in `shatter-go/protocol/build_tags.go`) extends `build.Default.BuildTags` with the value parsed out of `GOFLAGS=-tags=...` so a file gated on an active tag is no longer pre-flagged as `BuildTagExcludedError` / `not_supported`.
2. The **`go/packages` loader** (`shatter-go/loader/loader.go`) passes `os.Environ()` as the load `Env`, so `go list` itself reads `GOFLAGS=-tags=...` and emits the file in `pkg.Syntax` / `pkg.GoFiles` for the analyzer to walk.

Files gated on tags the user did **not** opt into still surface as `*BuildTagExcludedError` from the analyzer and `not_supported` from the handler, which is what the Rust core's `batch_analyze` path soft-skips (see str-8amu). No protocol-visible knob is added — the env-knob shape matches the precedent set by vendor (`str-nm5e`) and `go.work` (`str-b66s`) where the toolchain already provides the configuration surface.

Regression coverage lives in `shatter-go/protocol/build_tags_test.go`, including a `GOFLAGS=-tags=mytag` activation case and unit coverage of the `GOFLAGS` parser for the `-tags=` / `--tags=` / bare-flag forms.

## cgo

**Permanent non-goal.**

Shatter's concolic engine reasons symbolically using Z3, which does not extend across the C ABI. A function that calls into `import "C"` cannot have its C-side effects symbolically tracked, and attempting to analyze around cgo calls would either silently drop constraints (producing unsound tests) or pretend cgo-calling functions are pure (producing misleading coverage claims). The Go frontend will detect and refuse cgo-bearing functions at analysis time rather than attempt a partial model.

## `_test.go` files

**Permanent non-goal.**

Shatter's charter is to *generate* tests for production code. Existing tests are not analysis targets: their purpose is to assert behavior, and re-analyzing them would produce tests-of-tests, which is not a goal of the tool. Skipping `_test.go` is intentional, not an oversight, and will not be revisited.

## Unexported (package-local) functions

**Implemented.** Tracking: `str-z06h`.

The Go analyzer surfaces every top-level function declaration regardless of visibility — `FunctionAnalysis.Exported` and `DiscoveredTarget.Visibility` are honest tags, not gates. The filter is on the CLI side: `shatter scan` and `shatter run` skip unexported functions by default and include them when invoked with `--all` (`shatter-cli/src/args.rs`). The package-local wrapper code in `shatter-go/wrapper/` already calls unexported same-package targets, so the opt-in path is a one-flag change.

Default behavior — omit. Unexported functions are usually intermediate helpers rather than independent public surfaces, and including them by default expands the explore budget without a clear acceptance signal. The scan summary reports both the exported count and the count of unexported functions intentionally omitted, with a hint to re-run with `--all`.

Opt-in behavior — include. `shatter scan --all <path>` (and `shatter run --all <path>`) discovers, instruments, and explores every top-level function in the analyzed packages, exported or not. Coverage and branch reports list private targets alongside exported ones; the wrapper compiles them as same-package calls.

Out of scope — exploring unexported functions **from outside their defining package**. Go's visibility model makes this impossible without source-level edits, and Shatter intentionally does not synthesize callers in foreign packages to defeat it.

Fixture: `examples/go/private-funcs/`. See its README for the two invocation lines.

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
