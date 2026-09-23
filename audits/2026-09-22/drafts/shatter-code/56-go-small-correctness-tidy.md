# Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and uses backtick raw strings, dead assignments, unchecked lock-PID writes

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | go,cleanup,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qo1.12, str-qwua7.32 |
| source findings | frontend-go-14, frontend-go-15, prior-22 |

<!-- body -->
## Problem

Grouped small Go frontend defects.

## Current code facts / evidence

- `shatter-go/instrument/visitor.go:130-139` appends `makeLineRecordCall(line)` unconditionally; synthetic call_enter/call_exit resolve to line 0 → artifacts show `lines_executed: [0, 0, 4, 7, ...]`.
- `shatter-go/instrument/executor.go:226-231` emits ``json.Unmarshal([]byte(`%s`), &vals)`` (JSON inside a Go raw string: a backtick in a mock value breaks compilation) and `:299` unchecked `json.Unmarshal(retvals[idx], &retVal)`.
- `shatter-go/launcher/launcher.go:391` `_ = anchorImport`; `shatter-go/build/builder.go:277` `_ = fresh`; unchecked `fmt.Fprintf(lockFile, pid)` at builder.go:188 and launcher.go:616.

## Acceptance criteria

- No line-0 records (skip line <= 0); visitor test asserts it.
- Generated mock JSON embedded via strconv.Quote; generated code checks Unmarshal errors; test compiles a mock whose return value contains a backtick.
- Dead assignments removed; PID write errors handled.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-go-14, frontend-go-15, prior-22 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qo1.12, str-qwua7.32
