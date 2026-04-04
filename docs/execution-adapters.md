# Execution Adapters

Long-term architecture reference for framework-specific execution extensions in
Shatter.

## Status

This document defines the intended durable architecture for execution adapters.
It is not a statement of current user-facing behavior.

- [`SPEC.md`](../SPEC.md) remains the authority for implemented command behavior.
- [`PROTOCOL.md`](../PROTOCOL.md) remains the authority for implemented wire
  format behavior.
- This document defines the subsystem boundaries, extension model, heuristics,
  and issue-shaping rules that future implementation work should follow.

Use this document when:

- designing or reviewing framework-specific execution support
- deciding whether a new capability belongs in `shatter-core` or a frontend
- decomposing execution-adapter work into tracker issues
- evaluating whether a proposal composes cleanly with future extensions

## Why This Exists

Shatter works best when a target behaves like a plain exported function with a
direct call signature and a self-contained runtime. That assumption breaks down
for many real-world targets:

- TypeScript React hooks are not meaningful plain functions. They depend on a
  hook dispatcher, may require rerenders, and often return callbacks whose
  behavior is the real subject under test.
- TypeScript browser-facing code may depend on `window`, `document`,
  `localStorage`, `matchMedia`, `import.meta.env`, or project-specific module
  resolution.
- Go HTTP handlers do not communicate primarily through return values; they
  communicate through request/response side effects and framework contexts.
- Rust `async fn` values do not execute when called; they produce `Future`
  values that must be polled to completion under an async runtime.
- Rust and Go framework handlers often expose framework-owned extractor or
  context types instead of ordinary serializable parameters.

These cases require special handling, but Shatter is a general-purpose tool. The
core architecture must support such cases without baking framework semantics
into `shatter-core`.

The execution-adapter model exists to solve that tension.

## Design Goals

1. Keep `shatter-core` framework-agnostic.
2. Let language frontends own framework semantics.
3. Support multiple extension types, not only React hooks.
4. Support composition between extensions such as module resolution, browser
   globals, and framework-specific invocation models.
5. Make heuristics explainable and overridable.
6. Preserve the direct-call fast path for plain functions.
7. Give future TypeScript, Go, Rust, and other frontends one consistent model.
8. Provide a design that can be decomposed into small, independently landable
   issues.

## Non-Goals

- Do not introduce framework-specific enums or dispatch logic into
  `shatter-core`.
- Do not assume every special case should auto-apply.
- Do not force one extension mechanism to solve unrelated concerns such as
  output rendering, static analysis, and test generation.
- Do not make React the template for every future extension. React hooks are
  one important proving ground, not the universal shape.

## Core Terms

### Execution adapter

A language- or framework-specific extension that changes how a target is
recognized, prepared, invoked, observed, or provided with runtime context.

Examples:

- `ts/react-hooks`
- `ts/browser-dom`
- `go/http-nethttp`
- `rust/async-tokio`

### Execution profile

The ordered list of adapters and options selected for one target.

An execution profile may be:

- explicitly configured by the user
- auto-constructed from transparent recognizers
- suggested by semantic recognizers or runtime failures

### Recognizer

Frontend-owned logic that inspects a target and emits candidate adapter hints.
Recognizers may use static signals, project signals, or runtime failure signals.

### Transparent adapter

An adapter that mainly restores the expected runtime environment without
changing the target's semantic meaning.

Examples:

- tsconfig path resolution
- `import.meta.env` emulation
- a simulated browser global environment

Transparent adapters may auto-apply when heuristics are strong and the adapter
is low-risk.

### Semantic adapter

An adapter that changes the execution model of the target itself rather than
only supplying missing ambient environment.

Examples:

- React hook runner
- HTTP handler harness
- async runtime + extractor/response harness

Semantic adapters should default to suggest-only unless explicitly configured or
the user has enabled an opt-in policy for that adapter family.

### Invocation model

The generic description of how a target should be executed.

The default invocation model is a direct function call. Special targets may
advertise an adapter-owned invocation model with synthetic parameters or a
scenario schema.

### Scenario

A structured description of multi-step execution when one plain call is not
enough to model the target faithfully.

Examples:

- mount a hook, then invoke a returned callback
- construct an HTTP request, then inspect a response
- poll a future to completion inside a runtime

## Architecture Summary

The execution-adapter architecture has three layers.

### 1. `shatter-core`

`shatter-core` owns generic orchestration concepts only:

- config parsing and merge behavior
- generic protocol fields
- generic invocation metadata
- generic adapter hint surfacing
- generic auto-apply policy evaluation
- generic rendering of selected adapters and hint explanations

`shatter-core` must not know what React, Gin, Tokio, Axum, or browser globals
mean.

### 2. Language frontends

Each frontend owns:

- an adapter registry
- a recognizer registry
- translation from generic profile descriptors to executable plans
- framework-aware runtime setup and harness logic
- framework-aware observation normalization

The TypeScript, Go, and Rust frontends may each expose different adapters while
sharing the same generic architecture.

### 3. Adapter implementations

An adapter is a frontend-local unit that participates in one or more execution
roles. Adapters may be built in or loaded from a frontend-local registry, but
their semantics stay within the language frontend boundary.

## Stable Architectural Rules

These rules are long-term constraints on the subsystem.

1. Adapter identifiers are opaque to `shatter-core`.
2. Adapter identifiers are namespaced by language family, for example
   `ts/...`, `go/...`, and `rust/...`.
3. Adapter selection must be explainable to the user through recorded reasons.
4. Unsupported adapters must fail explicitly rather than silently degrading to a
   misleading direct-call path.
5. The absence of adapters must preserve current plain-function behavior.
6. Transparent adapters and semantic adapters must remain distinct categories
   for policy purposes.
7. New framework-specific behavior belongs in an adapter or recognizer, not as
   ad hoc code paths in the generic executor.

## Execution Profile

Execution profiles should be configurable in the same hierarchical style as
other `.shatter/config.yaml` behavior, with both defaults and per-function
overrides.

Illustrative shape:

```yaml
defaults:
  execution_profile:
    adapters:
      - id: ts/module-resolution/tsconfig-paths
      - id: ts/react-hooks
        apply: suggest
      - id: ts/browser-dom
        options:
          implementation: happy-dom

functions:
  "web/src/hooks/useTeamSwitch.ts:useTeamSwitch":
    execution_profile:
      adapters:
        - id: ts/react-hooks
          apply: required
        - id: ts/module-resolution/tsconfig-paths
          apply: required
```

Illustrative descriptor fields:

- `id`: opaque adapter identifier
- `apply`: `required`, `auto`, `suggest`, or `disabled`
- `options`: adapter-local opaque option payload

`shatter-core` should merge these descriptors generically and preserve order.
Only the frontend interprets their meaning.

## Adapter Hints

Recognizers should emit adapter hints during analysis. Hints are generic
descriptors that explain what the frontend believes is relevant about the
target.

Illustrative shape:

```json
{
  "adapter_id": "ts/react-hooks",
  "confidence": "high",
  "auto_apply_policy": "suggest",
  "why": [
    "imports react",
    "calls useCallback in exported target",
    "exported target name starts with use"
  ],
  "requires": [
    "ts/module-resolution/tsconfig-paths"
  ],
  "conflicts": [],
  "metadata": {
    "returned_callable": true
  }
}
```

Required generic fields:

- `adapter_id`
- `confidence`
- `auto_apply_policy`
- `why`

Optional generic fields:

- `requires`
- `conflicts`
- `metadata`

The frontend may emit richer metadata, but `shatter-core` should treat it as an
opaque JSON value.

## Recognition And Heuristics

Recognizers are the long-term mechanism for identifying special execution
cases.

### Signal classes

Recognizers may combine signals from several sources:

- signature signals
- imported modules or package names
- concrete framework types or traits
- body-level API usage
- file- or project-level metadata
- runtime failure signatures observed during execution probes

### Signal strength

Recognizers should classify evidence as:

- strong
- medium
- weak

Weak naming signals alone are not enough for semantic auto-apply.

Examples:

- Strong: exact `func(http.ResponseWriter, *http.Request)` signature.
- Strong: Rust `async fn` plus `tokio::spawn` usage.
- Medium: React import plus hook API calls inside an exported function.
- Weak: exported name starts with `use`.

### Recognition precedence

The long-term policy should be:

1. Explicit config
2. Explicit disable rules
3. Transparent auto-apply with strong evidence
4. Semantic suggestion with strong evidence
5. Runtime failure-driven suggestions

Runtime failures may upgrade visibility of a hint, but they should not silently
change semantic adapter selection unless the user has enabled a project-level
policy allowing that adapter family to auto-apply.

### Transparent vs semantic policy

Transparent adapters may auto-apply when:

- evidence is strong
- the adapter is low-risk
- the frontend can explain the decision

Semantic adapters should default to suggestion when:

- they replace the direct invocation model
- they require synthetic scenarios or framework-owned state
- there is material risk of exploring the harness instead of the target

### Runtime failure hints

Runtime failures are valuable recognizer inputs. Examples:

- `Invalid hook call` suggests `ts/react-hooks`
- `window is not defined` suggests `ts/browser-dom`
- unresolved aliased imports suggest `ts/module-resolution/tsconfig-paths`
- `Cannot use import.meta` or missing env values suggests `ts/import-meta-env`
- “no reactor running” suggests `rust/async-tokio`

Runtime failure hints must remain suggestions unless the adapter is classified
as transparent.

## Adapter Roles

An adapter may implement one or more roles. Roles are generic; their semantics
remain frontend-local.

### Resolution adapter

Influences how modules, packages, or symbols are resolved.

Examples:

- tsconfig path alias resolution
- Vite-specific import alias resolution
- framework virtual module provisioning

### Runtime adapter

Supplies ambient runtime objects, globals, or runtime services.

Examples:

- browser globals
- `import.meta.env`
- async runtimes
- request context or framework state containers

### Preparation adapter

Owns target-specific harness shaping or other pre-invocation preparation work
that cannot be expressed as plain generic instrumentation.

Examples:

- a hook runner wrapper around a React hook target
- an HTTP harness wrapper around a framework handler
- future-specific glue needed before polling or response normalization

### Invocation adapter

Owns how the target is actually executed when plain direct invocation is not the
right model.

Examples:

- mount a React hook runner
- build an HTTP request and response harness
- poll a future to completion

### Observation adapter

Normalizes framework-specific outcomes into Shatter's generic observations.

Examples:

- HTTP status, headers, and body become return-like or side-effect-like outputs
- hook rerender and action traces become structured observations
- framework-specific panic/error wrappers are normalized into generic errors

### Lifecycle adapter

Owns longer-lived framework state that must survive beyond one function call or
must be cleaned up deterministically.

Examples:

- DOM lifecycle setup and cleanup
- async runtime setup and teardown
- framework app-state setup for request handlers

The existing setup/teardown model is the preferred generic substrate for
lifecycle adapters whenever it fits.

## Execution Plan Pipeline

The frontend should translate a selected execution profile into an ordered
execution plan.

Recommended high-level order:

1. recognition
2. adapter selection
3. resolution setup
4. runtime setup
5. preparation
6. invocation planning
7. target execution
8. observation normalization
9. lifecycle cleanup

This is an execution order, not an issue dependency order.

Resolution must run before runtime setup when module loading depends on adapter
provided virtual modules or alias rewriting.

Observation must run after invocation and before final response assembly so the
frontend can preserve a generic wire format.

## Invocation Models

Long-term, `FunctionAnalysis` should be able to describe more than one
invocation shape.

Recommended generic categories:

- `direct`
- `adapter`

Illustrative shape:

```json
{
  "kind": "adapter",
  "adapter_id": "ts/react-hooks",
  "synthetic_params": [
    { "name": "hook_args", "type": "array" },
    { "name": "actions", "type": "array" }
  ],
  "scenario_schema": {
    "kind": "hook_callable_return"
  }
}
```

`shatter-core` should understand only:

- whether the target is direct or adapter-owned
- what synthetic parameters exist
- whether a scenario schema exists

It should not understand the semantics of React hooks, HTTP requests, or async
executors.

## Why React Hooks Need This Architecture

React hooks are one of the clearest examples of why direct invocation is not a
sufficient general model.

A custom hook may:

- require a live hook dispatcher
- require rerenders after state updates
- return a callback that performs the interesting work
- depend on context providers
- schedule effects

That means there are at least three distinct concerns:

- module and environment resolution
- runtime emulation of hook semantics
- invocation of the hook or returned callbacks

These concerns should not be collapsed into one React-specific code path in the
generic TypeScript executor.

### Hook-specific recognition

Strong and medium signals for `ts/react-hooks` include:

- imports from `react`
- calls to hook APIs such as `useState`, `useReducer`, `useCallback`,
  `useMemo`, `useEffect`, `useLayoutEffect`, `useContext`, or `useRef`
- exported target name matches the `useX` convention

### Hook-specific invocation models

Two hook invocation modes are important:

- `callable_return`: mount the hook, then call the returned function
- `scenario`: mount the hook, apply a sequence of actions, rerender as needed,
  and observe the result

The callable-return fast path should be the first prove-out because it supports
common hooks such as a hook that returns an event handler or mutation callback.

### Hook-specific observation

Observation should capture:

- hook arguments
- returned callback arguments
- rerender count
- effect scheduling and flush points
- final return value or thrown error

The goal is to describe hook behavior, not merely the behavior of a harness.

## Browser Runtime Adapters

Browser runtime support should remain a separate adapter family from React.

This separation matters because:

- some plain TypeScript functions need browser globals but not React
- some hooks need React semantics but no DOM
- browser support may also matter for non-React frontend frameworks

Useful TypeScript adapter families include:

- `ts/browser-dom`
- `ts/import-meta-env`
- `ts/module-resolution/tsconfig-paths`
- `ts/react-components`
- `ts/react-hooks`

`ts/browser-dom` should supply globals and browser services.
`ts/react-hooks` should own hook execution semantics.
These adapters must compose cleanly but remain independent.

## Why Go Cases Need This Architecture

Go framework cases require special handling because the meaningful contract is
not "call a function with JSON-like values and inspect its return."

### `net/http`

Classic handlers use:

```go
func(w http.ResponseWriter, r *http.Request)
```

Special handling is required because:

- the primary output is written through `http.ResponseWriter`
- headers, status code, and body are side effects, not return values
- the request includes URL, query, headers, body, cookies, and context
- middleware and helpers often rely on request-owned state

The correct execution model is request/response scenario construction and
observation, not direct arbitrary parameter generation for interface values.

### Gin and similar frameworks

Gin handlers often take `*gin.Context`.

Special handling is required because:

- `*gin.Context` bundles request, response, route params, abort state, and
  framework helpers
- the handler contract is tightly coupled to framework-owned context behavior
- response semantics are encoded through framework helpers such as JSON writers
  and abort methods

This is a semantic adapter problem, not a plain value-generation problem.

### Other Go candidates

The same architecture should support future Go adapters such as:

- `go/http-echo`
- `go/http-fiber`
- `go/context-runtime`

## Why Rust Cases Need This Architecture

Rust framework cases require special handling for the same structural reason:
the execution contract is framework-owned rather than direct and plain.

### Async functions

Calling an `async fn` does not run the function body to completion. It creates a
future that must be polled by an executor.

Special handling is required because:

- direct invocation returns a `Future`, not the semantic outcome
- many async targets require timers, task spawning, channels, or IO resources
- the runtime may matter to correctness, especially for Tokio-specific code

The minimal semantic adapter is therefore not just "call the function"; it is
"run the future to completion under an executor."

### Tokio-specific async code

Tokio-bound code may require:

- a runtime handle
- cooperative task scheduling
- timer support
- runtime-owned IO resources

That makes `rust/async-tokio` a runtime adapter with possible invocation and
observation implications.

### Axum, Actix, and similar handlers

Framework handlers often use extractor types and responder conversion.

Special handling is required because:

- the natural input is an HTTP request plus app state, not raw extractor
  values
- the natural output is an HTTP response or responder conversion
- many handlers are async and therefore also depend on a runtime

This means one target may require multiple composed adapters, such as:

- `rust/async-tokio`
- `rust/http-axum`

## Adapter Composition

Composition is a first-class requirement.

### Ordered composition

Profiles are ordered lists because adapter effects are not commutative.

Recommended order classes:

1. resolution adapters
2. runtime adapters
3. invocation adapters
4. observation adapters
5. lifecycle adapters around setup/cleanup boundaries

The frontend may internally normalize order, but user-visible reporting should
show the effective order clearly.

### Dependencies and conflicts

Adapters may declare:

- `requires`
- `conflicts`

Examples:

- `ts/react-hooks` may require `ts/module-resolution/tsconfig-paths` for some
  projects.
- one browser adapter implementation may conflict with another.
- one HTTP framework adapter may conflict with a different handler adapter for
  the same target.

The frontend should reject invalid combinations before execution begins.

### Composition matrix

Each frontend should maintain a tested composition matrix for supported adapter
families. Adding a new adapter without composition tests is not sufficient for
long-term maintainability.

## Configuration Guidance

Execution-adapter configuration should follow the same principles as the rest of
`.shatter/config.yaml`.

Guidelines:

- allow defaults and per-function overrides
- preserve explicit order
- preserve opaque adapter-local options
- let users explicitly disable a recognizer-selected adapter
- keep generic config generic; avoid framework-specific schema in core

Adapter-local options belong in the adapter descriptor payload and should remain
opaque to `shatter-core`.

## Protocol Guidance

The protocol should gain only generic fields needed to carry execution-adapter
state.

Suitable generic protocol additions include:

- selected execution profile descriptors
- adapter hints from analysis
- generic invocation model metadata
- generic explainability fields

The protocol should not add framework-specific message variants such as
`react_hook_execute`, `gin_request_execute`, or `tokio_execute`.

Those semantics belong behind generic request and response fields interpreted by
the frontend.

## Current Implementation Direction

At the time this document was written, the TypeScript frontend already contains
an ad hoc React-specific shim in [`shatter-ts/src/react-shim.ts`](../shatter-ts/src/react-shim.ts)
and executor-side special handling in
[`shatter-ts/src/executor.ts`](../shatter-ts/src/executor.ts).

That code should be treated as migration material, not as the target
architecture. The long-term direction is:

- move ad hoc frontend-specific logic behind adapter registries
- move recognizer logic into explicit recognizers
- preserve only generic hooks in executor infrastructure

This rule applies equally to future Go and Rust framework support.

## Compatibility And Rollout Rules

1. No adapter selection must preserve the plain-function path.
2. Unsupported adapter identifiers must fail clearly.
3. Transparent auto-apply must remain explainable and overridable.
4. Semantic adapters should require explicit enablement or user-accepted
   suggestion by default.
5. New adapter families should begin with one prove-out implementation and a
   fixture corpus before expanding to many frameworks.
6. Once behavior is implemented, `SPEC.md` and `PROTOCOL.md` must be updated to
   describe current behavior.

## Testing Requirements

Every adapter family should land with:

- recognizer tests
- execution tests
- failure-mode tests
- composition tests
- explainability tests

Fixture strategy should prefer minimal, framework-focused targets rather than
large application reproductions.

Examples:

- one hook returning a callback
- one DOM-dependent helper
- one `net/http` handler
- one Gin handler
- one `async fn`
- one Axum handler

## Deciding When To Add A New Adapter

Add an adapter when one or more of the following are true:

- the target's real contract is not a plain direct call
- the target requires framework-owned runtime state
- the correct output is encoded through side effects or responder conversion
- the runtime environment is recurring and framework-specific enough to merit a
  reusable abstraction

Do not add an adapter when a smaller generic improvement is sufficient.

Examples of changes that may belong in the generic frontend instead:

- better plain module resolution
- better generic serialization
- improved generic error reporting
- better baseline side-effect capture

## Issue Decomposition Guidance

Execution-adapter work should be decomposed in this order:

1. generic substrate
2. one frontend substrate
3. one adapter prove-out
4. one adjacent adapter that composes with it
5. follow-on framework families

Recommended issue categories:

- generic execution-profile substrate
- protocol and config pass-through
- frontend registry substrate
- recognizer infrastructure
- one framework adapter implementation
- one composition adapter implementation
- fixture corpus and composition matrix

Avoid oversized issues such as:

- "Add React support"
- "Support all web frameworks"
- "Make special cases work"

Prefer end-to-end slices such as:

- "TS analyze reports adapter hints for React hooks"
- "TS executor can run callable-return hooks"
- "Go net/http handler adapter normalizes status and body"
- "Rust async invocation model runs futures to completion under Tokio"

## Suggested Initial Adapter Families

TypeScript:

- `ts/module-resolution/tsconfig-paths`
- `ts/import-meta-env`
- `ts/browser-dom`
- `ts/react-components`
- `ts/react-hooks`

Go:

- `go/http-nethttp`
- `go/http-gin`
- `go/context-runtime`

Rust:

- `rust/async-basic`
- `rust/async-tokio`
- `rust/http-axum`
- `rust/http-actix`
- `rust/wasm-bindgen-browser`

These are examples, not a closed list.

## Long-Term Maintenance Rules

When adding or revising adapters:

- keep generic core fields generic
- keep frontend-local semantics frontend-local
- record recognizer reasons in user-visible form
- test composition explicitly
- update this document when subsystem invariants change
- update `SPEC.md` and `PROTOCOL.md` when implemented behavior changes

This document is the durable reference for the subsystem boundary and extension
model. It should evolve slowly and deliberately.
