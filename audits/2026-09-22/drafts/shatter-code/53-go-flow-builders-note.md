# NOTE on str-qwua7.35: four Go SymExpr builders; instrument/flow*.go is unreachable and CLAUDE.md names it as the ite mechanism

| field | value |
|---|---|
| action | **append comment to existing issue `str-qwua7.35`** (no new issue) |
| suggested priority for target | P2 |
| source findings | frontend-go-04 |

<!-- body -->
**Audit 2026-09-22 note** (findings: frontend-go-04)



### Current code facts
- `deadcode ./...` in shatter-go: instrument/flow.go (snapshot, mergeFlowMaps, symExprsEqual) and flowwalk.go (walkStmtsForFlow, applyStmtToFlow, applyAssignToFlow, applyDeclToFlow, applyIfToFlow, flowLHSName) unreachable.
- Runtime constraints come from `extractConstraint` without flow: visitor.go:334, 350, 440; mcdc.go:81, 93.
- The analyzer re-implemented flow (analyzerFlowMap, protocol/analyzer.go:1963-2110, 2171-2260, commit 34eb92c8); a fourth builder `buildGoLoopSnapshotSymExpr` (protocol/loop_body_states.go:227-335) resolves params before flow (analyzer does the reverse) — ordering unverified.
- `shatter-go/CLAUDE.md:52-62` lists flow.go/flowwalk.go as steps 1-2 of the ite mechanism.

### Proposed changes / acceptance additions
- Delete instrument/flow*.go; one flow-aware builder with a resolver hook in a leaf package used by analyzer, instrument and loop snapshots.
- Thread the flow map into instrument's transformIfStmt; rapid property: static condition == runtime constraint.
- Fix CLAUDE.md's ite section.
