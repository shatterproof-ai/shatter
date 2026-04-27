# Go Frontend v2 — Kapow Re-run Validation

**Issue:** str-hy9b.J3
**Date:** 2026-04-27
**Target:** `~/project/kapow` (Go API + tools, 499 .go files in api/, 2633 total)
**Shatter build:** main @ `24d6b4f6` (post str-1giz, str-hy9b.F4, str-hy9b.F5 module landing)

## Outcome

**Validation incomplete.** Two P1 regressions surfaced by this re-run abort the scan
before any meaningful before/after numbers can be collected. Both regressions
were introduced by Go-frontend redesign work (`str-hy9b.*` series and `str-1giz`)
and must be fixed before this validation can produce the comparison required by
the issue's acceptance criterion.

| | v1 baseline (per str-hy9b.J3 description) | v2 re-run (this attempt) |
| --- | --- | --- |
| Empty `.md` reports | 108 | not measured (scan aborted) |
| `internal-package` errors | 191 | not measured |
| `undefined` errors | 161 | not measured |
| Syntax errors | 56 | not measured |
| Total Go targets discovered | — | discovery did not complete |

The original baseline numbers come from a v1 run captured at
`/home/ketan/project/kapow/shatter-artifacts/source-code.md` (timestamp
2026-04-16): 833 discovered targets, 0 succeeded, 823 skipped, 10 failed.

## Regressions surfaced

### str-s3s4 — Scan aborts on NotSupported from frontend (P1)

Introduced by **str-1giz** (skip generated files by default).

`shatter-go/protocol/handler.go` now returns `NotSupported` when an analyze
request targets a generated file. `shatter-core/src/batch_analyze.rs:174-198`
treats *any* `ResponseResult::Error` — including `NotSupported` — as fatal,
aborting the entire batch analyze step.

```
Error: batch analyze failed: frontend error for /home/ketan/project/kapow/api/graph/generated/generated.go:
  frontend error (NotSupported): generated files are skipped by default
```

The pre-existing `_test.go` skip works only because file-listing filters
`_test.go` files **before** they reach `batch_analyze`. Generated-file detection
runs **inside the frontend handler**, so the file is sent to the analyze stage
and the `NotSupported` response kills the run.

**Fix preference:** treat `NotSupported` as a soft skip in `batch_analyze.rs`
(log warning, omit from registry, continue). Symmetric with how the scan
orchestrator already tolerates per-file failures elsewhere.

Workaround used here: `--exclude` patterns covering known generated paths
(`*.pb.go`, `*_gen.go`, `zz_generated*.go`, `generated.go`, `**/api/graph/generated/**`).
The workaround unblocks the first regression but immediately exposes the second.

### str-gq7c — `buildUnOp` emits binary-op names for unsupported unary tokens (P1)

Introduced earlier in the **str-hy9b** series (analyzer SymExpr emission).

`shatter-go/protocol/analyzer.go:1297-1314` `buildUnOp` handles only
`token.SUB` (neg) and `token.XOR` (bitwise_not) explicitly, then falls through
to `tokenToOp()` — which is the **binary** op map. For any other unary operator
the function emits a binary-op name as the unary variant, producing a payload
the Rust core cannot deserialize.

Concrete failure: `&x` (address-of) produces
`SymExpr{Kind:"un_op", Op:"bitwise_and"}` because `token.AND` maps to
`bitwise_and` in `tokenToOp`. Rust expects unary variants in
`{not, neg, bitwise_not, type_of, typeof}`:

```
Error: batch analyze failed: failed to deserialize frontend response (11437 bytes):
  unknown variant `bitwise_and`, expected one of `not`, `neg`, `bitwise_not`, `type_of`, `typeof`
hint: invalid JSON payload; first 200 chars: {"protocol_version":"0.1.0",...
```

Affected unary tokens (any `UnaryExpr.Op` other than `SUB` and `XOR`):

| Go token | Source | Currently emits | Should emit |
| --- | --- | --- | --- |
| `token.AND` | `&x` (address-of) | `bitwise_and` | `unknown` |
| `token.MUL` | `*x` (deref) | `mul` | `unknown` |
| `token.ADD` | `+x` (positive) | `add` | `unknown` |
| `token.ARROW` | `<-x` (receive) | varies | `unknown` |
| `token.NOT` | `!x` (logical not) | `not` | `not` (accidentally correct) |

Triggered by virtually every Go file in kapow's `api/` tree (address-of is
ubiquitous in Go). This regression alone makes any non-trivial Go scan
unviable.

**Fix preference:** switch on `expr.Op` explicitly in `buildUnOp`. For SUB →
neg, XOR → bitwise_not, NOT → not. For unsupported ops, return
`SymExpr{Kind:"unknown"}` rather than fabricating a fake op name.

## What ran successfully before abort

- Discovery / file listing: passed for ~2,500 files after `--exclude` patterns
  trimmed generated files and worktrees.
- Header-detection skip (str-1giz): correctly identified
  `api/cmd/genenum/main.go`, `api/graph/model/models_gen.go`,
  `api/graph/generated/generated.go`, `api/internal/search/field_name_gen.go`
  as generated. The skip mechanism itself works as designed; the bug is the
  downstream handling of the `NotSupported` response.

## Follow-up issues filed

| ID | Title | Priority |
| --- | --- | --- |
| `str-s3s4` | Scan aborts on NotSupported from frontend (str-1giz regression) | P1 |
| `str-gq7c` | Go frontend buildUnOp emits binary-op names for unsupported unary tokens | P1 |

## Items deferred (originally requested)

The issue description asked the validation to surface follow-up issues for
`_test.go`/cgo/vendor interactions. Those classes were not reached: the scan
aborted in the analyze stage before encountering their representative files.
A future re-run, after the two P1 regressions land, should:

- exercise representative `_test.go`-only packages (kapow has several)
- exercise cgo-dependent packages (search via `import "C"`)
- exercise vendor-resolved imports if any are present (kapow does not
  appear to vendor at the repo root, but check tools/ subtree)

## Recommended sequence

1. Fix `str-s3s4` and `str-gq7c` (separate small PRs each; both have small
   blast radius and clear test surfaces).
2. Re-run str-hy9b.J3 once both fixes land. The same `shatter scan` command
   used here should then complete.
3. If after the fixes the kapow numbers do not show the expected reduction
   in `internal-package`/`undefined` errors, file additional issues against
   the Go frontend's analyzer / packages-based loader — those reductions are
   the substantive AC of J3.

## Reproduction

```bash
shatter scan ~/project/kapow --language go \
  --exclude "**/.claude/**" \
  --exclude "**/.shatter/**" \
  --exclude "**/shatter-artifacts/**" \
  --exclude "**/.worktrees/**" \
  --exclude "**/api/graph/generated/**" \
  --exclude "**/*.pb.go" \
  --exclude "**/*_gen.go" \
  --exclude "**/zz_generated*.go" \
  --exclude "**/generated.go" \
  --output /tmp/scan-report.md \
  --timing summary --timing-output /tmp/timing.json --timing-format json \
  --log-level warn
```

Without the `--exclude` flags the scan aborts at the first generated file
(str-s3s4). With the flags it aborts at the first file using `&` (str-gq7c).
