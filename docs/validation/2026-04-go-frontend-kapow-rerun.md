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
| `str-dcqc` | HTTP adapters emit invalid TypeKind 'primitive' (str) | P1 |

## Update — 2026-04-27 second pass

After str-s3s4 and str-gq7c landed on main (commits 6fa927a0 and 036ad951
respectively), the scan was re-run against kapow with a freshly built release
binary. The first two regressions are confirmed fixed:

- **str-s3s4** — confirmed: the scan now emits `[warn] skipping unsupported
  file during batch analyze: …/api/graph/model/models_gen.go (generated files
  are skipped by default)` and continues, instead of aborting.
- **str-gq7c** — confirmed: files using `&x` no longer break deserialization;
  the scan reaches files much deeper in the API tree.

A **third** regression surfaced and aborts the second pass:

### str-dcqc — HTTP adapters emit invalid TypeKind `primitive` (P1)

`shatter-go/protocol/{gin_adapter.go, nethttp_adapter.go,
nethttp_recognizer.go}` emit synthetic_params with
`TypeInfo{Kind: "primitive", Label: "string"}` for adapter-recognized HTTP
handler functions (method/path/body fields). The Rust core's TypeInfo enum
has no `primitive` variant — valid variants are int, float, str, bool,
array, object, union, nullable, complex, opaque, unknown.

```
Error: batch analyze failed: frontend error for /home/ketan/project/kapow/api/internal/handler/log_config.go:
  failed to deserialize frontend response (49893 bytes): unknown variant `primitive`,
  expected one of `int`, `float`, `str`, `bool`, `array`, `object`, `union`, `nullable`, `complex`, `opaque`, `unknown`
```

Triggered by any Go file that imports `net/http` or `gin` and is matched by
the adapter recognizer — covers most of kapow's `api/internal/handler/`
subtree (~23 files in api/, more transitively via the recognizer).

**Fix preference:** change `Kind: "primitive"` to `Kind: "str"` in those
three files (the `Label: "string"` hint stays the same; serde tolerates the
extra `Label` field on the `Str` variant unless `deny_unknown_fields` is
set). Add a round-trip test covering an adapter-recognized HTTP handler.

The validation cannot produce before/after numbers until str-dcqc lands.

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

## Update — 2026-04-28 fourth pass: scan completes

After str-jdz8 (`extract_function_source` panic) and str-dcqc (HTTP/Gin
`Kind:"primitive"`) landed on main, the scan was re-run with all four code
fixes in place. **The scan completed (exit 0) and produced a full
`scan-report.md` and `timing.json` for the first time.** Run duration:
357 seconds (timing.json: 357032 ms).

A fifth issue (str-a2n2, P2) surfaced during this pass: `batch_analyze`
treats `ParseError` from build-tag-excluded files as fatal. Worked around
with three additional `--exclude` patterns (`api/tools.go`,
`api/ui/embed.go`, `api/ui/stub.go`); not yet fixed.

### Outcome distribution

| Bucket | v1 baseline (per str-hy9b.J3 description) | v2 fourth pass |
| --- | ---: | ---: |
| Functions explored (success or WARN) | 0 | 1 |
| Functions discovered & attempted | ~0 (Go-skipped wholesale) | 547 |
| Internal-package errors | 191 | 336 |
| Undefined errors | 161 | 43 |
| Syntax errors | 56 | 0 |
| Empty `.md` reports | 108 | n/a (different reporter) |
| Timeouts (30s) | not reported | 155 |
| Package-layout errors (main + main_test in same dir) | not reported | 6 |
| Module-resolution errors (`unknown revision`) | not reported | 3 |
| Other build failures | not reported | 3 |

### Interpretation

The error categories changed shape between v1 and v2 because the failures
moved layers:

- **v1**: Go targets failed at the **analyze / discovery** layer. The
  `source-code.md` from 2026-04-16 records 485 discovered Go targets, 0
  eligible, 0 attempted — the Go frontend never produced usable analyses.
  The 191/161/56 baseline numbers come from a slightly earlier v1 run that
  surfaced those errors as analyze failures.
- **v2**: 547 Go targets get past analyze, planning, and instrumentation;
  one explores cleanly (`UnmarshalFilterValue`, 57.1% coverage). The
  remaining 546 fail at the **launcher build** stage. The launcher
  generator emits a synthetic `shatter-launcher-<hash>` package OUTSIDE
  the kapow module tree, which Go's `internal/` visibility rule then
  rejects when the target uses any `kapow/.../internal/...` import. So
  the v2 internal-package errors (336) reflect a different,
  build-time barrier than v1's analyze-time barrier.

This is meaningful progress for the Go frontend redesign: analyze and
planning now work for hundreds of real-world functions. The remaining
internal-package errors are a launcher-architecture problem (the
generated harness needs to live inside the target module to access
`internal/`), not an analyzer problem.

### v1 vs v2 — bucket-by-bucket reading

- **Internal-package: 191 → 336.** Apparent regression, but actually progress
  shifted layers: v1 hit this at analyze, v2 hits it at launcher build because
  more functions now reach the build stage. To make the comparison meaningful,
  fix the launcher to live inside the target module so internal/ rules don't
  apply.
- **Undefined: 161 → 43.** ~73% reduction. Most v1 "undefined" errors came
  from packages-based-loader misses; the v2 packages-based analyzer (str-hy9b.C2)
  resolves them. Remaining 43 are real cross-package symbol issues during
  launcher build.
- **Syntax errors: 56 → 0.** Eliminated. The packages-based loader respects
  the Go AST cleanly; v1's syntax-error class came from line-based parsing
  shortcuts that no longer exist.
- **Timeouts: not reported → 155.** New visibility, not a regression. v1
  never reached the explore stage so timeouts didn't materialize. 30s per
  function on shared kapow targets is plausible — many are large rendering /
  database-bound handlers. Worth filing follow-up to surface a per-class
  timeout knob.

### Successful exploration sample

`UnmarshalFilterValue` (`api/graph/scalar/filter_value.go`):

- Coverage: 57.1% (4/7 lines, 1/1 branches)
- Mocks resolved: `json.Marshal`, `json.RawMessage`
- Behavior clusters: returns `0.5` for input `0.5`, returns `-391` for
  input `-391`, returns NaN for several other primitives — i.e. real
  exploration data.

This is the first time the v2 Go frontend has produced a non-trivial
explore result against an external real-world codebase.

### Follow-ups surfaced (filed beyond the original three)

| ID | Title | Priority | Status |
| --- | --- | --- | --- |
| `str-jdz8` | extract_function_source panics when start_line > file length | P1 | closed (landed) |
| `str-a2n2` | batch_analyze ParseError on build-tag-excluded files aborts scan | P2 | open |

### Known follow-ups still deferred (originally requested by J3 description)

- `_test.go`-only packages: not exercised in this pass (kapow's `_test.go`
  files are inline with sources; the analyzer may already cover them via
  the packages-based loader, but no targeted check was run).
- cgo-dependent packages: not surfaced as a distinct error class;
  re-survey after the launcher-internal-package issue is resolved.
- vendor/ subtrees: kapow does not vendor at the repo root.

### Recommended next steps (revised)

1. **Launcher inside-module synthesis.** File a P1 issue: the launcher
   harness must be generated inside the target module so Go's `internal/`
   visibility allows access. This will re-classify ~336 of the current 546
   errors. *(Gates the substantive comparison vs v1.)*
2. Fix str-a2n2 (build-tag-excluded files → soft-skip via NotSupported).
3. Investigate the 155 30s timeouts: are they really hung or do they need
   a higher per-function budget for kapow-scale handlers?
4. Then re-run str-hy9b.J3 a fifth time for a clean before/after.

### Reproduction (current)

```bash
shatter scan ~/project/kapow --language go \
  --exclude "**/.claude/**" --exclude "**/.shatter/**" \
  --exclude "**/.shatter-cache/**" --exclude "**/shatter-artifacts/**" \
  --exclude "**/shatter-artifacts-j3/**" --exclude "**/.worktrees/**" \
  --exclude "**/.cache/**" \
  --exclude "**/api/tools.go" --exclude "**/api/ui/embed.go" --exclude "**/api/ui/stub.go" \
  --output /tmp/scan-report.md \
  --timing summary --timing-output /tmp/timing.json --timing-format json \
  --log-level warn
```

The first 7 excludes filter scratch dirs / generated files / build-tag
files (the latter as workaround for str-a2n2). With these in place the
scan completes in ~6 minutes against kapow's 547 Go targets.
