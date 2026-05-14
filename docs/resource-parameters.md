# Resource Parameters

This guide is for users running Shatter against functions whose parameters are
not plain JSON-like values. Examples include database connections, LSP clients,
HTTP framework contexts, open files, sockets, async runtimes, and framework
state containers.

Shatter works best when a target can be called directly with generated values
for ordinary data parameters: strings, numbers, booleans, arrays, objects, and
language-specific shapes the frontend knows how to reconstruct. Resource
parameters need a different treatment because they are usually live process
state, OS handles, framework-owned values, or values with cleanup requirements.

## How Shatter Classifies These Targets

When Shatter cannot safely construct a parameter, the scan or explore output may
classify the target as unsupported. A common Go report looks like this:

```text
param "client" -> inner -> lsp.Client
type with no constructor - has no exported constructor or factory function
```

That message means Shatter followed the parameter shape, looked through the
pointer, and found a concrete type it cannot synthesize generically. It is not
asking for random input data. It is telling you that execution needs a real
resource setup path or a smaller testable API boundary.

## Choose A Strategy

Use the smallest strategy that preserves the behavior you want to explore.

| Situation | Use | What it does |
|-----------|-----|--------------|
| The function should not be explored directly because the resource is out of scope | `opaque_types` or function skip | Records that the type or target is intentionally unsupported |
| The function needs files, environment variables, seed data, or a background service, but its parameters are still constructible | setup files | Prepares surrounding state before execution |
| A parameter can be produced by a known frontend-side generator or runtime-value registry | generators / runtime values | Lets the frontend substitute a value for that parameter |
| The target's real contract is framework-owned or resource-owned | execution adapter | Changes how the frontend prepares, invokes, observes, and cleans up the target |
| The useful logic can be separated from the resource | refactor to a small interface or pure helper | Lets Shatter explore the behavior without a live resource |

## Mark A Resource Opaque

Use `opaque_types` when a type should be treated as intentionally
unexecutable. This is useful for broad scans where a live resource would be
slow, side-effectful, non-deterministic, or outside the current validation
scope.

```yaml
opaque_types:
  - name: lsp.Client
    reason: "requires a live LSP subprocess"
  - name: DatabaseConnection
    reason: "requires a configured database"
```

This does not teach Shatter how to construct the value. It makes the report
more honest by classifying matching targets as unsupported instead of spending
iterations on invalid executions.

## Use Setup For Surrounding State

Setup files prepare process, file, or project state before function execution.
They are appropriate when the target still has ordinary parameters, but needs a
workspace, config file, environment variable, fixture data, or service endpoint.

```yaml
defaults:
  setup: "./setup/go-workspace.go"
  setup_level: function
  setup_timeout: 10

functions:
  "internal/search/query.go:BuildQuery":
    setup: "./setup/search-fixtures.go"
```

Setup is not a portable way to pass arbitrary in-memory pointers into the
target. For example, a setup file can start an LSP server and return JSON
describing its endpoint or workspace, but a frontend adapter or generator must
still know how to turn that setup context into a `*lsp.Client` argument.

## Use Generators Or Runtime Values For Parameters

Generators are useful when the frontend can substitute a concrete value for a
specific type or parameter.

```yaml
defaults:
  generators:
    User: "./generators/user.ts"
  param_generators:
    authToken: "./generators/token.ts"
```

Use this path when the value can be reconstructed from generator output or from
a registered frontend runtime value. Some frontends also have built-in
runtime-value support for common parameters such as contexts or HTTP helper
values.

Do not use generators for arbitrary live resources unless the generator and
frontend also own the handle lifecycle. A JSON generator output alone cannot
carry an in-memory client pointer across process boundaries.

If a generator name is unknown to the frontend, Shatter should report that as a
planning or unsupported-parameter problem rather than silently falling back to a
bad value.

## Use An Execution Adapter For Resource-Owned Invocation

Use an execution adapter when direct invocation is the wrong model. Adapters are
frontend-owned extensions selected through `execution_profile`. They can own
preparation, runtime state, invocation, observation, and cleanup while the core
keeps the wire format generic.

```yaml
functions:
  "internal/backend/lsp/priming.go:PrimeWorkspace":
    execution_profile:
      adapters:
        - id: go/lsp-client
          apply: required
          options:
            command: gopls
            workspace: "./testdata/prime-workspace"
            language_id: go
```

The example above is an adapter shape, not a promise that every Shatter build
ships a `go/lsp-client` adapter. Adapter identifiers are interpreted by the
language frontend. If the selected frontend does not implement the requested
adapter, a required adapter should fail clearly.

An LSP adapter like this would need to:

- create or select a disposable workspace
- start the LSP server subprocess
- initialize the JSON-RPC session
- construct the `*lsp.Client`
- invoke the target with generated scenario inputs
- capture observations in Shatter's normal result shape
- shut the client down and clean temporary files

That work belongs in the frontend adapter or a project-specific custom
frontend, not in `shatter-core`.

## Prefer A Testable Boundary When Possible

For many resource helpers, the best fix is to keep the resource lifecycle
outside the target and make the target accept the smallest behavior it needs.

Instead of:

```go
func PrimeWorkspace(client *Client, root string, languageID string) error {
    // walks files and calls client.DidOpen(...)
}
```

prefer a boundary like:

```go
type FileOpener interface {
    DidOpen(path string, languageID string) error
}

func PrimeWorkspace(client FileOpener, root string, languageID string) error {
    // walks files and calls client.DidOpen(...)
}
```

That lets ordinary tests and Shatter-style harnesses exercise the interesting
logic with a small fake:

- directory traversal
- extension-to-language mapping
- maximum file count
- ignored open failures

The live LSP client can still be covered by a smaller integration test or by a
future adapter, but the pure priming behavior no longer depends on a subprocess.

## LSP Client Example

A function that accepts `*lsp.Client` usually cannot be explored by direct
parameter generation. A real client typically wraps a subprocess, pipes,
pending request channels, server capabilities, timeouts, and shutdown state.

For that shape, choose one of these outcomes:

1. **Skip or classify** the target with `opaque_types` if broad scan coverage is
   the goal and live LSP behavior is out of scope.
2. **Refactor to an interface** if the target mostly uses a small method such as
   `DidOpen`. This is the cheapest way to explore the branch logic.
3. **Build an adapter** if the goal is end-to-end LSP behavior. The adapter owns
   the live subprocess and cleanup.
4. **Use setup plus an adapter/generator** if the workspace or server can be
   prepared by setup, but the parameter still needs frontend-owned
   reconstruction.

Do not expect an exported constructor alone to make this target safe. A
constructor that launches external processes, depends on toolchain binaries, or
requires cleanup is a lifecycle contract, not just a value constructor.

## Current Documentation Boundaries

The implemented user-facing behavior is described by `SPEC.md` and the CLI
help. `docs/execution-adapters.md` describes the long-term adapter architecture
and is useful for designing new adapters, but it is not itself a guarantee that
an adapter identifier is available in every frontend.

When in doubt:

- use `shatter explore --help` and `shatter scan --help` for current CLI flags
- check scan output for adapter hints or unsupported reasons
- keep YAML config limited to adapter IDs and generators the active frontend
  actually supports
- prefer small pure boundaries for resource-adjacent logic
