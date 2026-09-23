# Go config loader walks up to `/` (stray /tmp/.shatter breaks tests; ancestor configs can widen policy.allow) and warns on the `defaults` key `shatter init` generates

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | go,config,tests,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-k7czv, str-uk8y, str-rs822, str-qwua7.58 |
| source findings | gates-03, frontend-go-07 |

<!-- body -->
## Problem

Two Go-frontend config bugs. (1) `findConfigFile` searches parents to the filesystem root with no stop, so any `.shatter/config.yaml` above the project is loaded: a stray `/tmp/.shatter/config.yaml` makes `TestLoad_MissingFile_ReturnsZeroFile` fail (open str-k7czv), and an ancestor config's `policy.allow` silently applies to every project below it. (2) `knownTopLevelKeys` lacks `defaults`, so every `shatter init` config makes the Go frontend warn.

## Current code facts / evidence

- `shatter-go/config/loader.go:228-249` `findConfigFile`: no module/VCS ceiling.
- `shatter-go/config/loader.go:413-425` `knownTopLevelKeys` contains only `functions` and `go_runtime_values`.
- `go test ./config/ -run TestLoad_MissingFile_ReturnsZeroFile -count=1` fails with `Warnings:[config /tmp/.shatter/config.yaml: ignoring unknown top-level key "defaults"]` (loader_test.go:668) whenever /tmp/.shatter/config.yaml exists; it was created by an implicit `shatter init` run with cwd=/tmp.
- The Rust CLI has its own discovery (see str-uk8y / str-rs822).

## Acceptance criteria

- Discovery stops at the nearest go.mod / go.work / .git (matching the CLI's boundary; document the chosen rule).
- The resolved config path is logged at debug level.
- `TestLoad_MissingFile_ReturnsZeroFile` is hermetic (explicit stop dir) and passes with a stray /tmp/.shatter present.
- `defaults` (and every other key `shatter init` writes) is accepted without warning; new cross-frontend test loads `shatter init` output in each frontend and asserts zero warnings.
- Test: an ancestor config's `policy.allow` is not applied to a project that has its own VCS root.

## Suggested approach

Add a stop boundary to `findConfigFile`, add `defaults` to known keys, add the parity test. Append a note to str-k7czv pointing here and close it when this lands.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Finding which process ran `shatter init` in /tmp (str-qwua7.58 covers implicit init).
- Size: S-M

## References

- Audit findings: gates-03, frontend-go-07 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-k7czv, str-uk8y, str-rs822, str-qwua7.58
