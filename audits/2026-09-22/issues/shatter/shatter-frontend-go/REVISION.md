# Revision: bucket `shatter-frontend-go` (2026-09-23)

Primary review: `crosscheck/shatter-frontend-go.codex.md` (Codex; 0 BLOCKER, 10 MAJOR, 2 MINOR, plus one MINOR listed as 13). Secondary: `crosscheck/shatter-frontend-go.md` (degraded same-runtime; 2 MAJOR, 7 MINOR). Evidence re-checked against the audit worktree (`/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`) and the live shatter bd DB (`bd show` on str-kzxt, str-dl2pj, str-k7czv, str-924ca, str-2fjn). Nothing filed (D6).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | MAJOR | 02/03: prefixed `go-tool/<tag>` tags are not needed; `continuous-*` are revision queries resolved to pseudo-versions | applied: tag-push AC and suggestion removed; docs AC says `@continuous-...` resolves to a pseudo-version; companion comment corrected | 02, 03 |
| 2 | MAJOR | `cargo test --test e2e_concolic_go` skips every case (`#[ignore]`) | applied: every draft now requires `task e2e-go` (runs `-- --include-ignored`, `Taskfile.yml:611-628`) plus the pasted cargo summary showing `0 ignored` and the new test names, with a direct-cargo fallback on a gate cache hit | 01, 04, 06, 07, 09, 11, 15, 16, 17, 18 (12 no longer touches E2E) |
| 3 | MAJOR | 01: green-release proof depends on Windows/aarch64 fixes but `blocked_by` is empty | applied by split: 01 now closes on a local relocation test; the release proof moved to new `go-release-relocation-smoke`, blocked by `go-harness-runtime-embed` and `release-publish-and-install-smoke` (which carries the Windows/aarch64/publish-guard deps) | 01, 14 |
| 4 | MINOR | 01: cwd change does not prove relocation; compiled-in path still exists on the runner | applied: 01's test builds from a temp copy and deletes it, uses a fresh `SHATTER_GO_WORKSPACE_ROOT`; 14 checks out to a different path, deletes `shatter-go/`, asserts the baked-in path is absent, and requires a negative-control run | 01, 14 |
| 5 | MAJOR | 04: E2E AC requires reaching the escaped-string arm whose concolic cause is out of scope | applied by split: 04's E2E covers quote/rune/byte; the escaped-string value is checked at analyzer level; the concolic miss is new `go-concolic-escaped-string-miss` (blocked by 04) | 04, 15 |
| 6 | MAJOR | 06: markerless standalone files (`loader.LoadFile`) still walk to `/` | applied: explicit 4-step rule (VCS root incl. `.git` file; else go.work/go.mod; else file's own dir only; never `/` or above `$HOME`) with a test per layout; fix must come from the rule, not a test-only marker | 06 |
| 7 | MAJOR | 08: atomic extract does not repair existing corrupt cache entries | applied: hash sidecar required for a cache hit; entries without one (all current caches) are treated as untrusted and replaced; tests for both | 08 |
| 8 | MAJOR | 08: sending `GITHUB_TOKEN` to manifest-provided URLs can leak it | applied: token only to an allowlist (`api.github.com`, `github.com`, https), never on redirects off it; tests with an injectable allowlist | 08 |
| 9 | MAJOR | 09: request timeout (30 s) = build timeout (30 s) and the request timeout taints the session (`frontend.rs:329-342`) | applied: 09 now blocked by cross-bucket `timeout-budget-invariant`; the "session survives" AC is a session test under default `--request-timeout` relying on that invariant; evidence cites `frontend.rs` and `args.rs:561-563` | 09 |
| 10 | MAJOR | 09: `Setpgid` process-group kill does not compile for the Windows target kept by D1 | applied: build-tagged Unix (process group) and Windows (Job Object) tree kill; `GOOS=windows` build + vet in the gate | 09 |
| 11 | MAJOR | 11: `Workspace.GoEnv` overrides `GOCACHE`; quiet-host success does not distinguish the fix | applied: evidence cites `workspace.go:199-214`; AC is a deterministic ordering test with a fake executor (serialize-then-fan-out, leader-failure), plus a real-frontend E2E with a fresh `SHATTER_GO_WORKSPACE_ROOT` asserting order from events, not timing | 11 |
| 12 | MAJOR | 10/13: implement-or-document left open | applied: 10 split (10 = record the divergence now, type task, S; new `go-connection-failures-impl` = implementation, P3, L, starting with a design note on the missing live-call hook). 13 now selects "refuse" as the deliverable; the probe becomes the red-state proof; narrowing the doc is named as a separate maintainer decision | 10, 13, 16 |
| 13 | MINOR | 12: four independent defects bundled | applied by split: 12 keeps the housekeeping pair (dead assignments, lock-PID writes); new `go-line-zero-records` and `go-mock-codegen-json` | 12, 17, 18 |

## Secondary (same-runtime) findings

| Sev | Finding | Action | Files |
|---|---|---|---|
| MAJOR | 01: `//go:embed` cannot reach `shatter-go/harness/` (separate module) | applied: AC is outcome-based; evidence explains the module boundary; embedded mirror + drift test (shown failing once) or a restructure | 01 |
| MAJOR | 06: "nearest go.mod" breaks `examples/go/*` nested modules that use the repo-root config | applied: VCS root wins over nested go.mod; test for a nested module (and `.git` file) finding the VCS-root config; E2E gate required | 06 |
| MINOR | 09: str-kzxt does not exist | applied: confirmed with `bd show str-kzxt` (no issue found); reference replaced by commit 9c39ad94 | 09 |
| MINOR | 02: CI proof cannot pass before landing | applied: pre-merge local `replace`-based resolution check in `task meta` plus a post-merge job (may use the commit SHA so it does not wait on release fixes) | 02 |
| MINOR | 13: `has_cgo_dep` wire field unmentioned | applied: evidence cites `types.go:59`, no core reader; AC chooses consumer or removal (regenerated bindings) | 13 |
| MINOR | 10: no Go live-call interception point shown | applied: confirmed none exists (only pre-execution `SideEffectClass` and call-site mock substitution); stated in 16, sized L | 10, 16 |
| MINOR | 04: `int` const type vs core typing unchecked | applied: evidence cites `analyzer.go:1555-1561` (`rune` → int; `byte` → `go_byte` complex); byte AC says to fix the `go_byte` sort if that is the cause | 04 |
| MINOR | 07: count needs date stamp / relative AC | applied: title/body "~57 (57 at 56c86168)"; AC uses the report taken at branch start, pasted with its SHA | 07 |

## Splits and new slugs

- `go-harness-runtime-embed` (01) -> + `go-release-relocation-smoke` (14, P1, blocked by 01 and cross-bucket `release-publish-and-install-smoke`).
- `go-rune-and-escape-literals` (04) -> + `go-concolic-escaped-string-miss` (15, P2, blocked by 04).
- `go-connection-failures-divergence` (10, now documentation only) -> + `go-connection-failures-impl` (16, P3 feature). 10 cites 16 by slug, not placeholder, because the filer files in file order and leaves forward placeholders unresolved.
- `go-small-correctness-tidy` (12, now housekeeping only, type chore) -> + `go-line-zero-records` (17) and `go-mock-codegen-json` (18). Finding ids re-attributed: 12 = frontend-go-15, 17 = frontend-go-14, 18 = prior-22.

No slug was removed or converted. `qwua7-35-four-go-builders` (05) is unchanged (no finding). 

## Cross-bucket dependencies introduced

- `go-build-timeout-ignored` blocked by `timeout-budget-invariant` (shatter-frontend-rust/03). That draft must keep its slug, and its invariant must cover Go execute requests that build (its out-of-scope line "Go/TS timeout defaults, beyond keeping the invariant general" is compatible).
- `go-release-relocation-smoke` blocked by `release-publish-and-install-smoke` (shatter-ci-workflows/03). That draft's smoke runs explore "from a checkout" on a same-path runner, which would not catch the relocation bug; 14 adds the steps to its smoke job.
