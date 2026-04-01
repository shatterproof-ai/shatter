# Frontend / Protocol Issue

Use this template when creating issues that touch frontend behavior, protocol messages, or parallel code paths. Fill in each section as the issue description when running `bd create`.

## Protocol-Visible Behavior

- Does this change the wire format or semantics of any protocol message? (yes / no)
- If yes, which commands or fields are affected:
- Is this change backward-compatible? (yes / no / N/A)

## Parity Impact

Which parallel code paths are affected? Check all that apply and confirm the parallel path is updated too.

- [ ] `buildSymExpr` (instrumentor.ts) — update `buildSymExprWithFlow` to handle the same node type
- [ ] `buildSymExprWithFlow` (instrumentor.ts) — update `buildSymExpr` to handle the same node type
- [ ] Random explorer (`shatter-core/src/explorer.rs`) — verify concolic orchestrator (`orchestrator.rs`) gets the same change
- [ ] Concolic orchestrator (`shatter-core/src/orchestrator.rs`) — verify random explorer (`explorer.rs`) gets the same change
- [ ] CLI wiring in `main.rs` — verify both `--concolic` and default explorer paths receive the new config or flag

## Default Behavior

- Is new functionality enabled by default? (yes / no)
- If no, what flag or config option enables it:

## Required Test Updates

- [ ] Unit tests added or updated
- [ ] Property tests (proptest / fast-check / rapid) added for any new public function
- [ ] E2E concolic tests verified (`cargo test --test e2e_concolic`)
- [ ] Conformance tests verified (`npx task conformance`) — required if protocol-visible
- [ ] Walkthrough updated (`demo/walkthrough.sh`) if the compact demo narrative changes
- [ ] Gauntlet updated (`demo/gauntlet.sh`) or equivalent targeted coverage added for non-demo CLI changes

## Contract Updates

- Are runtime contracts (`#[requires]` / `#[ensures]`) needed? (yes / no / N/A)
- If yes, identify the trust boundary that justifies them (Z3 FFI / subprocess JSON / cross-collection index):
- Confirm all three qualifying criteria are met: trust boundary, type gap, silent corruption (see `shatter-core/CLAUDE.md`)

## Notes

Keep scope narrow: one behavior end-to-end, not a layer-sliced stack. If acceptance criteria have more than three items, consider breaking into child issues under an epic.
