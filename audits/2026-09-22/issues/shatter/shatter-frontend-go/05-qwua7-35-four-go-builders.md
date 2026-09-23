---
slug: qwua7-35-four-go-builders
kind: note-to-existing
title: "NOTE on str-qwua7.35: four Go SymExpr builders; instrument/flow*.go is unreachable although CLAUDE.md names it as the ite mechanism"
priority: P2
type: note
labels: [go-frontend, symexpr, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.35
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.35: four Go SymExpr builders

Target: `str-qwua7.35` ("shatter-go: unify analyzer and instrument SymExpr builders over one shared type", open, P2).

Action: add the comment below with `bd comments add str-qwua7.35`. Priority stays P2. No new issue.

## Comment text

> **Audit 2026-09-22 note (finding frontend-go-04).** Re-verified at commit 56c86168. The scope of this issue is larger than two builders:
>
> **There are four Go SymExpr builders**
> 1. `shatter-go/protocol/analyzer.go:2171` `buildSymExpr` (static).
> 2. `shatter-go/protocol/analyzer.go:2201` `buildSymExprWithFlow` (static, flow-aware), with its own `analyzerFlowMap` (`analyzer.go:1963`) and walker; re-implemented in `protocol/` by commit 34eb92c8.
> 3. `shatter-go/instrument/symextract.go` `exprToSymExpr(WithFlow)` over a private `symExpr` (runtime).
> 4. `shatter-go/protocol/loop_body_states.go:294` `buildGoLoopSnapshotSymExpr`. It resolves params before flow (the analyzer does the reverse), handles only Ident/BasicLit/Binary/unary `-`/Paren, and keeps stale flow values after unsupported compound assignments such as `%=` or `<<=` (`visitGoLoopSnapshotAssign`, loop_body_states.go:227). Whether the ordering difference changes results is unverified.
>
> **The runtime builder never uses flow.** Instrument always calls `extractConstraint` with no flow map: `instrument/visitor.go:334, 350, 440` and `instrument/mcdc.go:81, 93`. `deadcode ./...` in shatter-go reports every function in `instrument/flow.go` (`snapshot`, `mergeFlowMaps`, `symExprsEqual`) and `instrument/flowwalk.go` (`walkStmtsForFlow`, `applyStmtToFlow`, `applyAssignToFlow`, `applyDeclToFlow`, `applyIfToFlow`, `flowLHSName`) as unreachable. Consequence: analyze reports `ite` conditions for flow-tracked locals while the runtime `branch_path` constraint for the same branch is `unknown`, so the solver cannot negate it.
>
> **Docs are wrong.** `shatter-go/CLAUDE.md:57-58` ("Ite SymExpr Parity Contract") lists `instrument/flow.go` and `instrument/flowwalk.go` as steps 1-2 of the ite mechanism.
>
> **Proposed additions to this issue's acceptance criteria**
> - Delete `instrument/flow.go` and `instrument/flowwalk.go`; keep one flow-aware builder with a param-resolver hook in a leaf package (e.g. `shatter-go/symexpr`) used by the analyzer, instrument and loop snapshots.
> - Thread the flow map into instrument's `transformIfStmt` so runtime constraints for flow-tracked locals match the static ones.
> - rapid property: for generated if-chains over params and flow-tracked locals, the static branch condition equals the runtime constraint.
> - Fix the CLAUDE.md ite section.
>
> The rune/escape literal bug (CHAR typed as `str`, analyzer `strings.Trim`) is filed separately as a small fix ahead of this refactor: **<id of go-rune-and-escape-literals>**. The dead-code sweep (**<id of go-dead-code-and-property-targets>**) leaves `instrument/flow*.go` to this issue.
