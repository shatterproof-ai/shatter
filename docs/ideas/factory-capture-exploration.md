# Factory Capture Exploration for Stateful Method-Producing Modules

## Why this exists

Some JavaScript and TypeScript modules do not primarily expose free functions at
module scope. Instead, they call a higher-order factory and return an object
whose interesting behavior lives behind function-valued properties. Zustand
stores are a common example:

- the module passes a callback into `create(...)` or `createWithEqualityFn(...)`
- the callback closes over helper functions and environment state
- the callback returns an object containing methods such as `addSearch`
- some helpers run during initialization
- other helpers are only reachable through returned methods

Shatter can often analyze these files syntactically, but that is weaker than
high-confidence execution coverage. If the goal were "get as close as possible
to maximum behavioral coverage without modifying the user's real source files",
the harness needs a different execution model.

This note describes that execution model.

## Problem statement

Direct function exploration works well when the target is a module-level export
or another symbol the harness can call directly.

It works poorly when:

- the interesting behavior is hidden behind a factory callback
- the returned methods depend on closure state
- helper functions are closure-local rather than module-level exports
- initialization performs meaningful work before any method is invoked
- the object being returned behaves like a state machine rather than a stateless
  function library

In the Zustand pattern, these are all normal.

## Target outcome

Without editing the user's real source file, the harness should be able to:

- detect that a module builds a stateful object through a known factory
- capture the initializer callback passed to that factory
- run the initializer in a controlled environment
- inspect the returned object
- identify function-valued properties as explorable targets
- execute those methods with generated inputs and controlled state
- attribute coverage back to the original source file and original helper paths
- explore initialization paths and method-call sequences, not just one-shot
  direct calls

This does not prove maximum coverage in the formal sense. It does produce a
much stronger approximation than "call the exported hook once" or "only cover
what happens at module import time."

## Core idea

Treat these modules as state-machine factories, not as ordinary files of free
functions.

The execution model becomes:

1. Intercept the higher-order factory call.
2. Capture the callback passed to the factory.
3. Re-execute that callback with controlled mocks.
4. Inspect the returned object.
5. Elevate returned methods into first-class exploration targets.
6. Explore both initial state and sequences of method calls.

That model is generic. Zustand is one adapter on top of it.

## Generic architecture

### 1. Detect candidate factory patterns

The analyzer should recognize modules whose primary behavior comes from a call
shape like:

- `factory(callback)`
- `factory<T>()(callback)`
- `factory(config)(callback)`

The generic question is not "is this Zustand?" The generic question is:

- does this module pass a callback into a higher-order function?
- does that callback return an object?
- does that object contain function-valued properties?

If yes, the file is a candidate for factory-capture execution.

### 2. Intercept the factory import at harness time

The harness should not change the user's real source file. Instead, it should
control how imports resolve inside the temporary instrumented copy.

For a supported factory library, the harness can replace the imported factory
with a shim that:

- accepts the same call shape as the real factory
- captures the callback passed into it
- records metadata about the call
- optionally returns a lightweight stand-in object so the module can continue to
  initialize

The important output is the captured callback, not the stand-in.

### 3. Execute the captured callback with a controlled runtime

Once the callback has been captured, the harness can run it directly with a
mocked runtime contract.

For a generic "factory returns object" API, this means:

- provide callback arguments with deterministic mocks
- provide environment shims such as `window`, `localStorage`, timers, and
  feature flags
- collect coverage while the callback runs
- capture the returned value

At this point the harness learns two things:

- what initialization code executes immediately
- what methods are present on the returned object

### 4. Promote returned methods into exploration targets

If the returned object contains:

- `addSearch(params) { ... }`
- `removeSearch(id) { ... }`
- `clear() { ... }`

the harness should treat each function-valued property as a target surface.

That means:

- each returned method gets a stable target identity
- each method gets parameter discovery and input generation
- each method can be executed repeatedly against a controlled store instance
- coverage is tracked per method and per sequence, not just per module import

This is the critical step that turns "we invoked the module" into "we explored
the behavioral API actually exposed by the factory result."

### 5. Explore sequences, not just single calls

Returned methods on stateful objects are rarely independent.

For high confidence, the harness should support:

- initialization-only runs
- one-step method calls
- multi-step method sequences
- alternate initial environment states
- alternate persisted state snapshots

Examples:

- empty `localStorage` then `addSearch`
- malformed `localStorage` then init
- populated state then `addSearch`
- repeated `addSearch` calls with equivalent and non-equivalent params

This turns the problem from "function call coverage" into "bounded state-machine
exploration."

### 6. Use coverage feedback to prioritize new sequences

If this mode is meant to maximize coverage, it should be coverage-guided from
the start:

- record which source branches fire during initialization
- record which branches fire during each returned method call
- prefer new seeds and new call sequences that hit unseen branches
- prune sequences that repeatedly produce identical coverage and state outcomes

This is especially important because method-sequence exploration grows quickly.
Without pruning, the search space becomes impractical.

## What this means for a Zustand store

Given a pattern like:

```ts
export const useRecentSearchStore = create<State>()((set, get) => {
  function loadFromStorage(): RecentSearch[] { ... }
  function summarizeSearchParams(params: SearchParams): string { ... }
  function saveToStorage(items: RecentSearch[]) { ... }

  return {
    recent: loadFromStorage(),
    addSearch(params) {
      const label = summarizeSearchParams(params);
      const next = [{ label }, ...get().recent];
      saveToStorage(next);
      set({ recent: next });
    },
  };
});
```

the harness should aim to cover:

- initialization paths through `loadFromStorage()`
- method paths through `addSearch(...)`
- helper paths reached transitively through `summarizeSearchParams(...)`
- persistence paths through `saveToStorage(...)`
- state transitions driven by `set(...)` and `get(...)`

Even if `loadFromStorage` is never directly callable as a free function, the
factory-capture model still covers it as initialization behavior.

Even if `addSearch` is not exported by name at module scope, the factory-capture
model can still treat it as a first-class behavioral target once the returned
object has been recovered.

## What is generic vs what is Zustand-specific

### Generic machinery

Most of the system is not specific to Zustand:

- recognizing higher-order factory patterns
- import interception in the instrumented copy
- callback capture
- controlled callback execution
- object inspection
- promotion of function-valued properties to targets
- coverage-guided sequence exploration
- branch and path attribution back to original source locations

This same machinery can apply to:

- store factories
- service builders
- command registries
- plugin registries
- dependency-injected object builders
- other libraries that return method objects from callbacks

### Zustand-specific adapter work

What is specific to Zustand is the runtime contract that makes execution
faithful enough to trust:

- recognize `create(...)` and `createWithEqualityFn(...)`
- support the curried invocation form used by Zustand
- provide realistic `set`, `get`, and `api`
- preserve Zustand update semantics
- preserve selector and equality behavior when relevant
- support common middleware such as `persist`, `devtools`, `immer`, and
  `subscribeWithSelector`
- provide a realistic storage model for persistence-oriented stores

So the overall architecture is mostly generic, but a production-quality adapter
needs library-specific semantic knowledge.

## Temporary AST transform as the stronger fallback

Factory capture gives high-confidence exploration of:

- initialization code
- returned methods
- any private helpers that those paths transitively execute

It does not automatically make every closure-local helper directly callable.

If the requirement is "get as close as possible to maximum coverage for helper
functions too," the next step is a temporary AST transform on the instrumented
copy only.

That transform can:

- identify closure-local helper functions inside the captured callback
- analyze whether they depend on closure variables
- generate wrappers or lifted aliases in the temporary module
- expose those wrappers to the harness for direct invocation

Important constraint:

- the transform changes only the generated harness copy
- the user's real source file on disk remains untouched

This is more powerful than pure factory capture, but also more risky:

- wrappers may accidentally change binding semantics
- lifted helpers may no longer observe exactly the same captured environment
- attribution becomes more complex because the callable wrapper is synthetic

Factory capture should come first. Temporary lifting should be reserved for
cases where indirect coverage is not enough.

## Coverage model: direct, indirect, and lifted

This problem becomes much clearer if coverage claims are split into three
categories.

### Direct behavioral coverage

The harness directly invokes a returned method from the factory result.

Example:

- instantiate store
- call `addSearch(params)`
- record branches hit inside `addSearch`, `summarizeSearchParams`,
  `saveToStorage`, and any other helpers it reaches

This is the strongest practical signal short of direct helper lifting.

### Indirect initialization coverage

The harness executes the initializer and records what runs during store
construction.

Example:

- instantiate store
- `loadFromStorage()` runs during `recent: loadFromStorage()`

This is still real coverage, but it is weaker than direct targeting because it
only sees the initialization path.

### Lifted helper coverage

The harness executes a synthetic wrapper around a closure-local helper in the
instrumented copy.

Example:

- generate a wrapper that calls the helper under a reconstructed closure context
- invoke it directly with generated inputs

This offers the broadest reach, but it carries the highest semantic risk.

Any reporting system should make these distinctions explicit instead of claiming
they are equivalent.

## What "maximum coverage" can realistically mean

For this class of code, "maximum coverage" should not mean "mathematically prove
all reachable paths have been explored." That is unrealistic for stateful,
environment-dependent JavaScript.

A more defensible definition is:

- initialization paths were explored under representative environment states
- all returned methods were discovered and exercised
- method-call sequences were explored up to a bounded depth
- environment-dependent branches were varied through controlled mocks
- closure-local helpers were covered indirectly where reachable
- optional temporary lifting was used for high-value helpers that would
  otherwise remain hidden
- the harness can justify why additional search did or did not produce new
  coverage

That is a practical "high confidence" target.

## Implementation phases

### Phase 1: Factory capture and returned-method discovery

Deliver:

- detect supported factory calls
- capture initializer callback
- run callback with mocks
- inspect returned object
- enumerate function-valued properties

This phase alone should materially improve coverage on stores like Zustand.

### Phase 2: Stateful method exploration

Deliver:

- create a reusable instance model for the returned object
- generate inputs for returned methods
- support bounded call sequences
- track coverage and state deltas per sequence

This phase turns the feature from "callable discovery" into real behavioral
exploration.

### Phase 3: Semantic adapters and middleware support

Deliver:

- robust Zustand semantics
- persistence-aware mocks
- middleware-aware execution
- better error reporting when semantic fidelity is insufficient

This is where the feature becomes trustworthy for real applications rather than
toy examples.

### Phase 4: Optional temporary helper lifting

Deliver:

- identify closure-local helpers inside factory callbacks
- synthesize wrappers in the temporary copy
- attribute coverage from wrappers back to original helper definitions

This phase should remain optional because it increases complexity and semantic
risk.

## Risks and failure modes

### Semantic drift from the real library

A fake `create(...)` that is too shallow may produce reassuring but incorrect
coverage. If the adapter does not match Zustand semantics closely enough, the
result is not trustworthy.

### Environment oversimplification

Stores often depend on:

- browser APIs
- persistence
- timers
- subscriptions
- middleware behavior

If these are mocked too weakly, coverage may be shallow or misleading.

### State explosion

Returned-method sequence exploration grows quickly. Coverage guidance and
sequence pruning are mandatory.

### Attribution ambiguity

If a helper was only hit during initialization, that should be reported
differently from direct method exploration. If a helper was reached through a
synthetic wrapper, that should also be reported distinctly.

## Success metrics

If implemented, the feature should be evaluated with metrics that reflect the
real goal:

- number of returned methods discovered per supported store file
- percentage of returned methods that become directly invocable targets
- branch coverage increase over import-only or export-only execution
- number of initialization-only helpers covered
- number of previously unreachable closure-local helpers covered via optional
  lifting
- coverage plateau behavior as sequence depth increases
- false-confidence rate from semantic mismatches in the adapter

## Recommendation

If this ever becomes a top-priority coverage problem, the most defensible path
is:

1. Implement generic factory capture.
2. Add a high-fidelity Zustand adapter.
3. Explore returned methods as stateful actions with coverage-guided sequences.
4. Add temporary helper lifting only for the cases where indirect coverage is
   still insufficient.

That gives the best balance of reach, fidelity, and safety without modifying the
user's underlying source files.
