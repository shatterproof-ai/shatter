# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

DEGRADED review: the Codex counterpart failed identity validation (exit 4), so this is a same-runtime (Claude) fallback review, read-only.

Scope: 2026-09-22 Shatter audit issue drafts, bucket shatter-frontend-go (13 drafts in BUNDLE.md). Claims were checked against the audit worktree at 56c86168 (/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22), the live bd DB, and a fresh `deadcode ./...` run in shatter-go.

## Overall

Evidence quality is high. I spot-checked most file:line citations and they hold: executor.go:112-133 runtime.Caller; the analyzer.go:2351 strings.Trim path; symextract.go CHAR typed `str`; findConfigFile walking to `/`; knownTopLevelKeys; deadcode printing exactly 57 lines, including reconstruct/, planner.Classify and 9 flow*.go functions; the go-tool main.go line numbers; the SHATTER_BUILD_TIMEOUT push in helpers.rs with no reader in Go; no connection_failures/runtime_crypto_boundaries in the Go Response; BuildWarmupGate only in scan_orchestrator.rs; the line-0 record emission; the backtick-embedded mock JSON; and fnHasCGoDep being signature-only. The problems are mainly with acceptance-criteria design and a few references.

## Findings

### MAJOR — go-harness-runtime-embed: the required `//go:embed` fix cannot work as written because of a module boundary
`shatter-go/harness/` is its own module (it has its own `go.mod`, `module shatter-harness`), and `shatter-go/go.mod` neither requires nor replaces it. `//go:embed` cannot match files in a directory that belongs to another module, and not only files reached via `..`. So no shatter-go package can embed `harness/go.mod` + `runtime.go` in place. The suggested "leaf package" would need a copied or generated mirror of the files, with a drift check, or the harness would have to be restructured. The AC hard-codes "go.mod and runtime.go are //go:embed'ed". Reword it to require the outcome (the runtime is available with no source tree), and name the copy/generate mechanism plus a drift test in the suggested approach.

### MAJOR — go-config-discovery-unbounded: the proposed boundary rule breaks existing nested-module layouts
The AC stops discovery at "the nearest directory containing go.mod, go.work or .git". This repo already relies on crossing a nested go.mod. For example, `examples/go/multi-file-service/go.mod` (and most other examples/go/* modules) have no `.shatter/` of their own and today resolve the repo-root `.shatter/config.yaml`. The nearest-go.mod rule would silently drop that config. Say which marker wins (VCS root is safer than module root). Add an AC that a config at the VCS root above a nested module is still found, or record the behaviour change and migrate the affected fixtures. The rule also has to agree with str-dl2pj, and the draft leaves that open ("or the rule str-dl2pj adopts"). That makes "done" depend on another issue's decision.

### MINOR — go-build-timeout-ignored: str-kzxt does not exist in the tracker
`bd show str-kzxt` returns "no issue found". The id appears only in commit 9c39ad94's message. Drop it from Related, or cite the commit only.

### MINOR — go-tool-module-path: the CI proof cannot pass before landing
The job runs the documented `go get -tool github.com/...@<ref>` against the public repo, so it can only pass after the rename is on main and a `<dir>/<tag>` tag has been pushed. State that it runs post-merge (release or continuous workflow). Alternatively, add a pre-merge variant that resolves the module from the local checkout, for example a `GOFLAGS=-modfile` temp module with `replace`, or `GOPROXY=off` plus a VCS file URL, so the module-path/directory check still gates the PR.

### MINOR — go-cgo-refusal-covers-bodies: the serialized `has_cgo_dep` field is not mentioned
`HasCGoDep` is part of the protocol (`protocol/types.go:59`, `json:"has_cgo_dep,omitempty"`), but nothing in shatter-core reads it. The draft says the only reader is planner.Classify, which is true only on the Go side. Mention the wire field. Under option (a) or (b), decide whether it is removed, kept, or given a consumer in the core.

### MINOR — go-connection-failures-divergence: option (a) assumes Go has a live-call interception point
The AC asks Go "mocks/adapters" to classify dial failures on outbound calls. The draft does not show that the Go frontend observes live outbound calls today (as opposed to substituted mocks). Add one sentence saying where that hook exists, or state that (a) first needs such a hook, which would move the size to L.

### MINOR — go-rune-and-escape-literals: the `int` const type is not checked against the core's typing
Before the fix lands, confirm that the core/solver treats a `rune`/`byte` param's sort as int, so that `{type:int}` is well-typed against `{param c}`. Cite where param types for rune/byte are mapped.

### MINOR — go-dead-code-and-property-targets: the census and counts need a date stamp
The count of 57 matches today, but it moves with every commit. Make the AC relative ("every function in the deadcode report at branch start") rather than tied to 57. The AC mostly does this already; the title should say "~" consistently.

No BLOCKERs. Duplicates look correctly handled: str-k7czv is to be closed as a duplicate, str-dl2pj is linked, and the str-qwua7.35 and str-qwua7.48 notes are right to go to existing issues rather than new ones. The notes on existing issues (go-tool-reopen-note, qwua7-35-four-go-builders) are accurate: the str-fl9g.2 acceptance text and close reason match the quoted text.

## Verdict

Ready to file after two AC fixes:
1. go-harness-runtime-embed: replace the literal `//go:embed` requirement with an outcome-based AC that accounts for the harness being a separate module.
2. go-config-discovery-unbounded: choose a boundary rule that keeps repo-root configs reachable from nested go.mod modules (for example, the VCS root), and add a test for that case.
3. Drop the nonexistent str-kzxt reference, and split the go-tool-module-path CI proof into a pre-merge variant and a post-merge variant.
