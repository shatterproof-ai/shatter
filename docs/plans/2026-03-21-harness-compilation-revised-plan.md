# Revised Plan: Semantic Harness Compilation for Rust and Go

## Purpose

This document replaces the architecture proposed in
`~/.claude/plans/replicated-noodling-axolotl.md` with a revised design based on
the strongest conclusions from two follow-on critiques:

- `docs/plans/2026-03-21-replicated-noodling-axolotl-review.md`
- `~/.claude/plans/harness-compilation-critique.md`

The revised design keeps the original goal:

- compile less
- execute many times
- avoid recompilation when only inputs change

But it changes the semantic boundary of the solution:

- compile inside the user's real crate/module context whenever possible
- minimize in-tree generated files
- externalize cache/build/temp state by lifecycle
- avoid import rewriting as a core mechanism

## Problem Statement

The current execute path recompiles too often.

Today:

- Rust generates a fresh harness project per execute call
- inputs are baked into source, forcing rebuilds for every iteration
- Go has the same general shape, though compile times are lower
- warm iteration cost is dominated by unnecessary regeneration and rebuild work

This is the wrong architecture for exploration. The correct model is:

- compile artifact once
- execute artifact many times with different requests
- invalidate only when code or integration state changes

## Design Goals

The revised design should satisfy these goals:

1. **Preserve semantic correctness**
   - Rust harnesses must compile with real crate visibility, features, macros,
     and module structure.
   - Go harnesses must preserve real package/module semantics.

2. **Keep in-tree footprint minimal**
   - Prefer one stable generated harness entrypoint per language or per crate,
     not one file per function.

3. **Separate storage by lifecycle**
   - Tool-owned cache/build state must not be mixed with preserved outputs.
   - Preserved outputs should be visible under `shatter-artifacts/`.

4. **Delegate compilation invalidation to native build tools**
   - Cargo and `go build` already know when recompilation is needed.
   - Shatter should only track what is unique to Shatter, especially
     instrumentation staleness.

5. **Never make users worse off than the current approach**
   - If the new path fails, fall back to the current direct-compilation path.

## Non-Goals

This plan does not attempt to:

- build a separate persistent Rust harness crate with import rewriting
- generate one binary per function
- rely on overlay or union filesystems
- optimize cold-starts beyond what profiling justifies

## Key Architectural Decisions

### 1. Inputs and mocks move to stdin

This remains the highest-value immediate change.

Implications:

- different inputs do not require source regeneration
- mocks do not require source regeneration
- one compiled harness can serve many execute calls

This is required for both Rust and Go.

### 2. Rust compiles semantically inside the user's crate

The harness must be part of the user's actual crate, not a separate crate that
depends on it.

The implementation provides two harness modes:

- **`bin_only` (default)**: generates a `[[bin]]` target inside the user's crate.
  Because a `[[bin]]` is a separate crate that depends on the library, it can
  access `pub` items but **cannot** name `pub(crate)` types in function
  signatures. Functions that internally use `pub(crate)` items work fine — the
  visibility boundary only affects what the harness source must name at compile
  time. Functions with `pub(crate)` signature types fail with a clear compiler
  error and fall back to the standalone path.

- **`crate_bridge` (opt-in)**: injects a feature-gated wrapper module into the
  library crate itself, enabling calls to crate-private functions. This mode
  can access `pub(crate)` items but requires modifying the user's `Cargo.toml`
  and `lib.rs` with feature-gated additions.

Reason for in-crate compilation (both modes):

- `crate::`, `self::`, and `super::` semantics remain intact
- macro expansion and feature gating match the real crate
- build-script and workspace behavior remain native

This explicitly rejects the original separate harness-crate approach.

### 3. Go compiles with real package semantics

Go should preserve package-level behavior rather than centering the design on
single-file copied modules split across packages.

Reason:

- unexported helpers are package-scoped
- package init behavior matters
- build tags and module configuration matter

This rejects the original `cmd/` plus `modules/` split as the primary design.

### 4. Use one stable dispatch harness, not one binary per function

The least intrusive correct design is one stable generated harness entrypoint
per language or per crate/package, with runtime dispatch based on the request.

This avoids:

- O(n) generated files
- O(n) build targets
- per-function target bookkeeping
- collision-prone naming schemes

### 5. Split storage by lifecycle

The design assumes the storage split described in `str-y3po` lands.

Storage classes:

- **In-tree stable harness entrypoint**
  - minimal generated source needed for semantic compilation

- **Tool-owned cache/build state**
  - reusable generated metadata
  - build outputs
  - temp compiler inputs when needed

- **Disposable temp output**
  - per-run scratch files
  - transient logs

- **Preserved user-facing outputs**
  - `shatter-artifacts/`
  - behavior maps
  - traces/exports users may want to keep

### 6. Let Cargo and `go build` own build staleness

Shatter should not try to replace native compilation invalidation logic with a
parallel custom system.

Use Shatter-side hashing only for:

- source instrumentation staleness
- dispatch table regeneration decisions
- cheap short-circuiting of unnecessary instrumentation work

Use Cargo / `go build` for:

- dependency changes
- toolchain changes
- feature changes
- manifest and lockfile changes
- true compilation staleness

## Revised Architecture

## Rust

### Harness shape

Generate one stable harness entrypoint in a toolchain-native location, such as:

- `src/bin/__shatter_exec.rs`

Alternative acceptable location:

- another single stable ignored bin path in the crate

The file should:

- read newline-delimited JSON requests from stdin
- dispatch by function identifier
- deserialize inputs and mocks
- invoke the target function
- serialize result JSON to stdout
- reset runtime state between requests

### Why one stable file

This is the least intrusive option by file count:

- one stable generated file
- easy single-pattern git ignore
- no per-function source file growth in the tree

### Dispatch model

The harness is generic and request-driven.

Dispatch keys should be stable identifiers derived from:

- crate-relative module path
- function name

The dispatch implementation may grow over time, but it remains a single file or
single logical generated unit.

### Manifest integration

If needed, add a single O(1) integration point to `Cargo.toml` so the harness
can build as a native bin target. Any such modification must be:

- minimal
- clearly marked as autogenerated
- atomic when written
- safe to leave in place

The harness integration should not create a second `Cargo.toml` under the
project root.

### Build outputs

Prefer native Cargo build behavior over custom build state management.

Acceptable approaches:

- use the crate's normal `target/`
- or set a dedicated external target dir if profiling shows that is materially
  better

This choice should be explicit and measured, not assumed.

## Go

### Harness shape

Use one stable dispatch harness preserving package semantics.

Depending on package layout, this may be:

- a stable in-module harness command
- or a flat generated package in external cache when that better preserves the
  current working behavior

What matters is semantic correctness, not directory purity.

### Requirements

- no structural split that loses access to unexported package symbols
- no recorder placement that crosses package boundaries incorrectly
- module configuration must resolve correctly, including replace directives

### Build outputs

Go build/cache state should live outside the project tree in a tool-owned cache
location, such as an XDG cache directory keyed by project identity.

## Rust Harness Mode Boundaries

The Rust frontend supports two harness modes, selectable via the `harness_mode`
field on execute requests:

| Mode | Default? | Visibility | Trade-off |
|---|---|---|---|
| `bin_only` | Yes | `pub` items only | No modifications to user's `lib.rs`; functions with `pub(crate)` signature types fail with a clear error |
| `crate_bridge` | No (opt-in) | `pub(crate)` and `pub` | Injects a feature-gated wrapper module into the library crate; can access crate-private functions |

**`bin_only` mode** generates a `[[bin]]` target (`src/bin/__shatter_exec.rs`)
that depends on the library. Because a `[[bin]]` is a separate crate in Rust's
module system, it can call any `pub` function but cannot name `pub(crate)` types
in function signatures (parameters or return types). Functions that *internally*
use `pub(crate)` items work fine — the visibility boundary only affects what the
harness source must reference at compile time.

**`crate_bridge` mode** adds a feature-gated module directly inside the library
crate, bypassing the crate boundary. This enables calling crate-private
functions but requires modifying the user's `Cargo.toml` and `lib.rs` with
clearly-marked, feature-gated additions.

When `bin_only` encounters incompatible functions, `check_bin_only_compatibility()`
in `executor.rs` produces a diagnostic listing every problem and suggesting
`crate_bridge` as an alternative.

## Standalone Files and Fallback

Standalone files with no parent `Cargo.toml` or `go.mod` cannot use the
in-crate/in-module path.

Standalone fallback behavior:

- build in a generated minimal project in XDG cache
  (`$XDG_CACHE_HOME/shatter/standalone-{lang}/`)
- use stdin-driven requests
- use debug builds by default unless profiling proves otherwise
- single-shot execution (no persistent subprocess)

This fallback is also used when `bin_only` mode encounters a build failure for
crate-resident files — the system logs the error, attempts the standalone path,
and surfaces both failures if the fallback also fails.

Because standalone files lack real crate/module integration, this fallback does
not inherit the semantic risks of the original separate-crate Rust design.

## Execution Model

### Baseline

For both Rust and Go:

1. Receive execute request.
2. Determine target function and source location.
3. Check whether Shatter instrumentation is stale.
4. Re-instrument only if needed.
5. Ensure harness dispatch source is current.
6. Build via native toolchain if needed.
7. Execute the compiled harness with request data over stdin.
8. Return result to core.

### Persistent subprocess (deferred)

Persistent subprocesses are a phaseable optimization, not a prerequisite.
**Status: deferred** — not yet implemented. The current architecture uses
single-shot harness execution per request.

If later enabled, the harness can remain running and process multiple requests
over stdin/stdout, reducing warm execution cost further. This should only be
added after basic compile-once/run-many behavior is working and measured.

## Staleness and Invalidation

### Shatter-owned staleness

Shatter should track:

- source content hash for instrumentation
- possibly a generator version for dispatch source generation

This tells Shatter whether it must:

- re-instrument
- rewrite the dispatch table/source

### Build-tool-owned staleness

Cargo / `go build` should decide whether the binary itself is stale based on:

- source changes
- dependency changes
- toolchain changes
- feature changes
- manifest/lockfile changes

This avoids reimplementing native build invalidation poorly.

## Failure and Fallback Strategy

The new architecture must not strand users behind new failure modes.

If the semantic harness path fails:

1. surface the real build error with context
2. log that fallback is being attempted
3. attempt the legacy direct-compilation path
4. if fallback also fails, surface both failures

This makes rollout safer and keeps the current path as a compatibility escape
hatch.

## Cleanup and Lifecycle

### Lifecycle-specific storage

Prepared harness state is scoped by lifecycle level:

- **Function-level**: `prepared_harnesses` map in the handler, keyed by
  `prepare_id` (SHA-256 of `file:function:sorted-mock-symbols`). Cleared on
  function-level teardown.
- **Session-level**: all prepared harnesses cleared on shutdown.

The `prepare` command pre-builds the harness binary so subsequent execute calls
skip compilation. This is distinct from the persistent subprocess optimization
(deferred) — prepare caches the *binary*, not a running process.

### Session end

- stop any persistent subprocesses if they exist (currently N/A — deferred)
- keep reusable cache/build state for future sessions

### Explicit cleanup

Provide a cleanup command or equivalent behavior that can:

- remove tool-owned cache for a project
- remove stable generated harness entrypoints if desired
- remove other generated integration state if it was added

### Orphan handling

On startup or rebuild:

- detect dispatch entries whose source files no longer exist
- prune stale generated entries
- recover cleanly from missing binaries or metadata

## Phased Implementation Plan

### Phase 1: stdin-based request passing

Scope:

- Rust harness reads inputs/mocks from stdin
- Go harness reads inputs/mocks from stdin

Value:

- removes per-iteration recompilation immediately

Stop gate:

- measure walkthrough step 10
- if this alone solves the timeout comfortably, later phases become optional
  optimizations

### Phase 2: cheap build-mode improvements

Scope:

- use debug builds by default where appropriate
- preserve warm compiler caches instead of deleting everything after each run

Value:

- improves cold and warm rebuild times with low architectural risk

### Phase 3: stable semantic harnesses

Rust:

- add one stable native harness entrypoint
- compile in real crate context

Go:

- add one stable semantic harness path preserving package behavior
- move tool-owned build/cache state out of the project tree

Value:

- removes the need for Rust import rewriting
- preserves crate/package semantics
- keeps in-tree generated file count low

### Phase 4: robust cache and cleanup behavior

Scope:

- lifecycle-specific storage handling
- recovery for missing generated files/binaries
- orphan pruning
- explicit cleanup path

### Phase 5: persistent subprocess optimization (deferred)

**Status: not yet implemented.** This phase is deferred until profiling data
from phases 1-4 justifies the added complexity.

Scope:

- optionally keep one harness process alive per project or per crate/package

Value:

- reduces warm execution cost further

Constraint:

- only do this if profiling shows process spawn overhead still matters after
  phases 1-4

## Verification Criteria

### Performance

1. Walkthrough step 10 completes within its timeout budget.
2. Repeated executes with different inputs do not trigger recompilation.
3. New function execution in a warm project benefits from native incremental
   compilation.
4. Cold-start timing improves materially over the current design.

### Semantic correctness

5. Rust functions with `pub` signature types compile and run in `bin_only` mode.
6. Rust functions with `pub(crate)` signature types compile and run in
   `crate_bridge` mode, or fail with a clear error and standalone fallback in
   `bin_only` mode.
7. Rust functions using `crate::`, `self::`, and `super::` structures compile
   and run without import rewriting.
8. Rust macro-heavy and feature-sensitive code paths are handled correctly or
   rejected explicitly.
9. Go functions depending on unexported package helpers across files compile and
   run.
10. Go package/module init behavior remains correct.

### Storage behavior

11. Preserved outputs are written under `shatter-artifacts/`.
12. Tool-owned cache/build state is not mixed with preserved outputs.
13. Disposable run output does not accumulate in preserved or cache locations.

### Failure handling

14. Missing generated files or binaries recover cleanly.
15. New-path build failures trigger fallback to the legacy path.
16. Users are not left worse off than before if the new path encounters edge
    cases.

## Why This Plan Is Better Than the Original

The revised plan keeps the original performance insight while fixing the main
architectural mistake.

Compared with the original design, this plan:

- preserves native semantic compilation context
- avoids Rust import rewriting
- avoids one-binary-per-function target explosion
- minimizes in-tree generated file count
- separates cache/temp/artifact lifecycles cleanly
- uses the native build tool for compilation staleness
- keeps a safe fallback path during rollout

## Final Recommendation

Adopt this revised design instead of the original persistent external harness
project design.

The right boundary is:

- semantic compilation inside the user's real crate/package
- minimal stable generated source in-tree
- everything heavy or disposable outside the tree
- preserved outputs under `shatter-artifacts/`

That is the best balance of correctness, performance, and low intrusiveness.
