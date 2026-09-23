# Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/...` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | go,distribution,install,docs,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-fl9g.2, str-wnyzy |
| source findings | frontend-go-02 |

<!-- body -->
## Problem

The Go tool wrapper's module path does not match its directory, so the documented install command fails. str-fl9g.2 was closed on 'landed on main' with exactly this command as its acceptance check.

## Current code facts / evidence

- `shatter-go-tool/go.mod:1` `module github.com/shatterproof-ai/shatter/go-tool`; no `go-tool/` directory ever existed (`git log --all -- go-tool/go.mod` empty); no root go.mod.
- `docs/distribution.md:59-71` documents `go get -tool .../go-tool/cmd/shatter@...`; Renovate regex at :110 uses the same path.
- `GOPROXY=direct go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@main` → 'module github.com/shatterproof-ai/shatter@main found (...), but does not contain package .../go-tool/cmd/shatter'.
- release.yml pushes no `go-tool/<tag>` tags for the nested module.

## Acceptance criteria

- Directory renamed to go-tool/ (or module path + docs + Renovate regex changed consistently).
- Nested-module tags handled (release.yml pushes go-tool/<tag>, or docs use pseudo-versions).
- CI job runs the exact documented `go get -tool` + `go tool shatter --shatter-wrapper-help` in a temp module.
- str-fl9g.2 reopened or linked as fixed-by this issue.

## Suggested approach

Rename is simplest; update all references (Taskfile meta, CI, docs).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-go-02 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-fl9g.2, str-wnyzy
