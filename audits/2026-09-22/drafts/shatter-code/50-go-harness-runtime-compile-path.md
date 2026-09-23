# Go frontend finds its harness runtime module via the compile-time source path: installed/relocated binaries fail every uncached execute

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | go,distribution,install,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-o650 |
| source findings | frontend-go-01 |

<!-- body -->
## Problem

`ensureHarnessRuntimeDir` locates `harness/go.mod` relative to the file path recorded by `runtime.Caller(0)` at build time. Release binaries are built on CI runners without embedding the module, so on user machines every uncached Go execute fails, mislabelled as `build_failed`.

## Current code facts / evidence

- `shatter-go/instrument/executor.go:109-133` `ensureHarnessRuntimeDir` uses runtime.Caller(0) and stats `<srcdir>/../harness/go.mod`; callers `build/instrumented_overlay.go:162`, `:819`.
- `shatter-go/instrument/api.go:16-20` doc says it 'materializes' the module; no `//go:embed` exists in non-test Go code.
- `.github/workflows/release.yml:175` builds with plain `go build -o ../staging/$GO_BINARY .` (no -trimpath, no embed); `shatter-cli/build.rs:163-205` likewise.
- Repro: build shatter-go in a copy, move the copy, drive handshake/analyze/execute(Double,[5]) → `instrumentation_failed`, outcome `build_failed` 'go build failed during harness compilation', message `stat .../harness/go.mod: no such file or directory`. Targets with a cached launcher still work (hides the bug on dev machines).

## Acceptance criteria

- harness go.mod + runtime.go are `//go:embed`ed and materialized once into `<workspace>/harness-runtime/<content-hash>/`.
- A missing runtime is reported as an infrastructure error, not build_failed.
- Test: build with -trimpath, execute a fresh target from outside the repo.
- release.yml smoke: run `shatter explore` on an example from a directory without the source tree.

## Suggested approach

Embed + materialize; add the relocation test.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-go-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-o650
