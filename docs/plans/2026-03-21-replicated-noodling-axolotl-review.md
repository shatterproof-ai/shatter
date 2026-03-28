# Review of "Cached Harness Compilation for Rust and Go Frontends"

## Purpose

This document is a red-team response to `~/.claude/plans/replicated-noodling-axolotl.md`.
It preserves the useful parts of the original diagnosis, critiques the weak
parts of the proposed architecture, and offers a revised counterproposal.

This review assumes one follow-on infrastructure change is accepted:

- Internal harness state, build cache, disposable temp output, and preserved
  analysis output are split by lifecycle instead of sharing a single
  mixed-purpose `.shatter/` directory.
- Preserved user-facing outputs live in `shatter-artifacts/`.

That storage split improves the operational shape of the system, but it does
not by itself solve the main semantic risks in the original plan.

## What the Original Plan Gets Right

The original plan correctly identifies the main performance problem:

- The current execute path recompiles too often.
- Baking inputs into generated source forces unnecessary rebuilds.
- Rebuilding from scratch for each execute call is structurally wrong.
- Moving request data to stdin is the highest-value immediate optimization.

The plan also correctly aims for these properties:

- compile less, execute more
- reuse warm compiler caches
- avoid per-iteration source regeneration
- preserve a request-driven runtime protocol

Those goals are still the right ones.

## Revised Assessment

With the storage split assumed to land, the original proposal becomes more
operationally reasonable, but the central Rust design is still weaker than it
appears. The biggest unresolved problem is semantic fidelity: the original plan
tries to compile instrumented code in a separate harness crate that depends on
the user crate, then recover meaning with import rewriting. That is not a safe
default for real Rust code.

The original plan also chooses the wrong granularity for several components:

- cache invalidation is keyed too narrowly
- build targets are too fine-grained
- Go compilation is treated too much like file-level work
- persistent generated sources are still too numerous

The right direction is not "persistent external harness project with per-function
binaries." The right direction is "semantic compilation inside the user's real
crate or module, with as little in-tree generation as possible and everything
heavy moved out of the source tree."

## Detailed Critique

### 1. The Rust harness crate model is semantically brittle

The original proposal compiles instrumented Rust files in a separate harness
crate with a path dependency on the user crate, then rewrites `crate::...` and
related paths to cross the dependency boundary.

That approach is fragile for common Rust code, not just edge cases.

Problems:

- Files often depend on private module structure, sibling modules, and
  non-exported helpers.
- `pub(crate)` is only one visibility failure mode; private items, local module
  shape, and `super::` structure are also common blockers.
- Macro expansion, feature gating, build-script behavior, and module placement
  can differ when code is moved out of the real crate tree.
- A Rust source file usually does not have the same meaning outside its
  original crate/module hierarchy.

Why this matters:

- The system will compile and execute correctly on simple demos while failing on
  ordinary real-world crate structure.
- The failure mode is architectural, not incidental. More rewriting logic will
  not fully fix it.

Conclusion:

- A separate harness crate should not be the primary Rust design.

### 2. Import rewriting is understated

The original plan describes Rust import rewriting as a mechanical `syn`-based
AST transform. That understates both complexity and residual risk.

Problems:

- Rewriting must handle `use`, expressions, types, nested paths, aliases,
  macros, `super::`, `self::`, and more.
- Correctly transforming syntax does not guarantee semantic equivalence after
  crossing a crate boundary.
- The cost of building and maintaining compiler-adjacent rewriting logic is
  high, especially when attached to a design that remains semantically weak even
  after rewriting succeeds.

Conclusion:

- Avoid designs that require import rewriting as a core mechanism.

### 3. One binary per function is the wrong build granularity

The original plan proposes one persistent binary per function.

Problems:

- It multiplies generated targets and cache bookkeeping.
- It increases name collision surface.
- It increases initial build work for files with multiple executable functions.
- It creates more invalidation and cleanup complexity than necessary.

Better direction:

- One stable dispatcher binary per language, per crate/module context, or at
  worst per source file/package.
- Dispatch at runtime based on the request payload.

Conclusion:

- Per-function binaries are too fine-grained for the value they provide.

### 4. Source-file-only hashing is too weak for cache validity

The original plan keys staleness primarily off the target source file hash.

Problems:

- A cached binary can become stale when dependencies change.
- Cargo/go manifests and lockfiles can change.
- Toolchain versions can change.
- Instrumentation logic can change.
- Harness generation logic can change.
- Runtime crates/packages can change.
- Build mode or feature flags can change.

Risk:

- The most dangerous failure mode is silent stale execution rather than a
  compile miss.

Conclusion:

- Cache keys must include more than source content.

### 5. The Go design is better than Rust, but still too file-centric

The original Go proposal is less risky than the Rust proposal because Go does
not have Rust's privacy model. It still overstates how independent a single Go
source file is.

Problems:

- Go packages, not files, are the semantic compilation unit.
- Package-private helpers often live in sibling files.
- `init` behavior may be package-wide.
- build tags, generated files, and package layout can make file-level copying
  non-equivalent to real package compilation.

Better direction:

- Cache and build at package granularity, not single-file granularity.

Conclusion:

- The Go side should preserve package semantics rather than centering the design
  on copied individual files.

### 6. The original `.shatter/` design mixed incompatible lifecycles

This point improves under the assumed follow-on storage split, but it is worth
stating explicitly because it affects the architectural direction.

Original problem:

- generated harness source
- build artifacts
- disposable temp output
- metadata
- preserved analysis outputs

were all effectively being placed under one tool-owned directory.

That is a weak design because those classes of data want different policies:

- cache: versioned, reusable, evictable
- temp output: disposable, run-scoped
- preserved outputs: durable, user-visible
- metadata: durable but tool-owned

Conclusion:

- The storage split is the right correction and should be treated as a
  prerequisite for any persistent caching design.

### 7. Concurrency and corruption handling are underspecified

The original plan mentions a single lock during compilation.

Problems:

- No atomic write strategy is defined for generated files or metadata.
- No stale lock recovery strategy is defined.
- No versioning scheme is defined for cache layouts.
- No clear behavior is defined if one process executes a binary while another
  rebuilds it.
- No corruption recovery path is defined for partial writes or interrupted
  builds.

Conclusion:

- Persistent cache designs need stronger write and versioning rules than a
  single `.lock` file.

### 8. Verification is too performance-heavy and not semantically adversarial enough

The original verification section is strong on timing checks and basic smoke
cases, but not strong enough on semantic fidelity.

Missing or under-specified cases:

- Rust files using sibling private modules
- Rust macro-heavy code
- Rust workspace/feature interactions
- Rust methods, generics, and async functions
- Go packages with package-private helpers in other files
- Go package-wide init behavior
- stale-cache invalidation after manifest or runtime changes
- behavior when generated files or binaries are manually deleted

Conclusion:

- A revised design must be validated against semantic stress cases, not just
  throughput targets.

## Revised Counterproposal

### Core idea

Keep semantic compilation inside the user's real crate/module context, but make
the in-tree footprint as small and stable as possible.

The least intrusive version that still preserves the goals is:

- one stable harness entrypoint file per language
- generated in a toolchain-native location inside the user's source tree
- generic/request-driven over stdin
- everything else moved out of the source tree

This is less intrusive than per-function generated files and more semantically
correct than a separate harness crate.

### Why this is the right tradeoff

This counterproposal optimizes for:

- semantic correctness first
- low in-tree file count
- easy git ignore behavior
- reuse of native compiler caching
- clear lifecycle separation outside the repo

The primary measure of intrusiveness here is number of added files, especially
files that are hard to ignore. A single stable generated harness file is the
best answer to that metric.

## Revised Architecture

### 1. Generate one stable harness entrypoint per language

Rust:

- Generate a single stable file such as `src/bin/__shatter_exec.rs`.

Go:

- Generate a single stable file such as `cmd/__shatter_exec/main.go`.

Properties:

- The file is stable in path and name.
- It is easy to ignore with one gitignore pattern.
- It does not multiply as new functions are executed.
- It stays small and mostly generic.

This is intentionally not one file per function.

### 2. Make the harness request-driven

The harness entrypoint should:

- read a request from stdin
- identify the target function
- deserialize inputs and mocks
- execute the target
- emit JSON to stdout
- reset runtime state between executions

This preserves the main win from the original plan:

- no recompilation for different inputs or mocks

### 3. Keep semantic compilation inside the real crate or package

Rust:

- Compile via the user's actual crate so name resolution, privacy, macros,
  features, and build behavior remain correct.

Go:

- Compile in the real module/package context rather than centering the design
  on copied individual files.

This removes the need for the original Rust dependency-boundary import
rewriting design.

### 4. Move everything heavy out of the source tree

With the storage split assumed to land, the revised layout should be:

- In-tree:
  - one stable harness entrypoint file per language
  - optional minimal stable manifest if needed

- External cache:
  - build outputs
  - reusable generated metadata
  - tool-owned cache state
  - compiler target directories

- External temp:
  - per-run scratch files
  - transient execution outputs
  - temporary logs

- Preserved artifacts:
  - `shatter-artifacts/`
  - behavior maps
  - preserved traces/exports intended for user inspection

This keeps the real source tree nearly clean while preserving the semantics of
native compilation.

### 5. Prefer one dispatcher binary over many function-specific binaries

The stable harness should dispatch dynamically instead of generating a new build
target per function.

Benefits:

- fewer generated files
- fewer build targets
- better cache locality
- easier cleanup
- lower collision risk

The runtime dispatch layer is more complex than a single-purpose generated
`main`, but that complexity is lower and more defensible than maintaining many
per-function binaries.

### 6. Use stronger cache keys

Cache validity should include at least:

- source fingerprint
- manifest/lockfile fingerprint
- harness generator version
- instrumentation version
- runtime version
- toolchain version
- build mode/feature set

If any of those change, the cache entry should be invalidated.

This is stricter than the original source-file-only hash design, but much
safer.

### 7. Define explicit write and versioning rules

Persistent cache/state handling should include:

- versioned cache directory layout
- atomic temp-file-plus-rename writes for metadata
- staged rebuild then atomic promotion where possible
- lock scoping for rebuild operations
- stale lock recovery
- safe behavior when generated binaries are missing or partially written

This is necessary to make a persistent design supportable.

## Alternatives Considered

### Separate harness crate with import rewriting

This is the original proposal's main Rust design.

Pros:

- keeps generated sources out of the user crate

Cons:

- semantically brittle for Rust
- requires import rewriting
- still fails on common crate-structure cases

Assessment:

- not recommended as the primary design

### Overlay or union filesystem

This could make generated files appear to be in the source tree without writing
them there.

Pros:

- low visible repo pollution

Cons:

- platform-specific
- operationally fragile
- poor default fit for cross-platform tooling

Assessment:

- viable only as a specialized technique, not the main design

### Copy the whole source tree

Pros:

- tools operate on a normal filesystem tree

Cons:

- expensive
- duplication-heavy
- complex invalidation

Assessment:

- viable but heavier than necessary

### Copy tree plus symlink original files

Pros:

- less duplication than full copy
- preserves a writable synthetic tree

Cons:

- still more complex than needed
- symlink behavior varies by platform and tool

Assessment:

- a more viable fallback than overlayfs, but still inferior to a single stable
  in-tree harness file

### Single stable in-tree harness file

Pros:

- best semantic fidelity
- minimal file count
- easy to ignore in git
- simplest operational model that preserves the goals

Cons:

- writes one generated file into the repo
- requires a generic runtime dispatcher

Assessment:

- recommended

## Revised Implementation Plan

### Phase 1: Immediate high-value changes

1. Move inputs and mocks to stdin for Rust.
2. Move inputs and mocks to stdin for Go.
3. Preserve warm compiler caches instead of deleting all build state after each
   execute.

This phase should be done before larger architecture changes because it delivers
substantial speedup with low semantic risk.

### Phase 2: Introduce stable harness entrypoints

Rust:

- add one stable harness file in a native bin location
- compile in the user's actual crate context

Go:

- add one stable harness file in a native command location
- compile in the real package/module context

The harnesses should remain generic and request-driven.

### Phase 3: Split lifecycle storage cleanly

Assuming the issue to separate storage lifecycles lands:

- keep only the stable harness entrypoint in-tree
- use external cache for build and reusable tool-owned state
- use external temp for scratch output
- use `shatter-artifacts/` for preserved outputs such as behavior maps

### Phase 4: Add robust invalidation and recovery

- implement stronger cache keys
- version cache layout
- add atomic metadata writes
- add corruption/missing-file recovery behavior

### Phase 5: Expand semantic coverage and verification

- add tests for private module structure in Rust
- add tests for package-wide behavior in Go
- add invalidation tests for manifest/runtime/toolchain changes
- add dispatcher tests for multiple target functions

## Revised Verification Criteria

### Performance

1. Walkthrough step 10 completes within its timeout budget.
2. Repeated executes with different inputs do not trigger recompilation.
3. Warm execution latency is materially below current behavior.
4. First compile in an already-warm project reuses native compiler cache.

### Semantic fidelity

5. Rust functions using sibling/private module structure still compile and run.
6. Rust functions using macros and external deps still compile and run.
7. Rust workspace/feature cases are handled or rejected explicitly.
8. Go functions that depend on package-private helpers in sibling files still
   compile and run.
9. Package/module init behavior remains correct.

### Cache validity

10. Changing manifests, lockfiles, runtime code, or instrumentation logic
    invalidates stale binaries.
11. Missing generated binaries or metadata recover cleanly.
12. Concurrent rebuild attempts do not corrupt cache state.

### Storage behavior

13. Preserved outputs are written under `shatter-artifacts/`.
14. Tool-owned cache/build state is not mixed with preserved outputs.
15. Disposable execution output does not accumulate in preserved or cache
    directories.

## Key Tradeoffs

### Why this is better than the original plan

- It preserves semantic compilation context instead of simulating it across a
  crate boundary.
- It reduces generated file count dramatically.
- It avoids Rust import rewriting as a core dependency.
- It uses lifecycle-specific storage instead of a mixed-purpose tool directory.
- It retains the main performance win from stdin-driven repeated execution.

### What it still costs

- One stable generated source file per language is still written into the user's
  source tree.
- The harness must be generic enough to dispatch multiple target functions.
- Cache management still needs robust invalidation and recovery logic.

These are real costs, but they are more controlled and more defensible than the
costs in the original design.

## Final Recommendation

Do not proceed with the original Rust design of a separate persistent harness
crate plus import rewriting as the default architecture.

Proceed instead with:

1. stdin-based execution requests
2. warm compiler cache reuse
3. one stable in-tree harness entrypoint per language
4. semantic compilation in the real crate/module context
5. externalized cache/build/temp storage
6. preserved outputs in `shatter-artifacts/`

This revised design is the best balance of performance, semantic fidelity, and
low intrusiveness.
