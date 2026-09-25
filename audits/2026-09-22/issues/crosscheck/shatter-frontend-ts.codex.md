# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** f01d23bf566dfe13fd5011a35f6ceaa0c544393687b9f776b2505f91c6444ddd
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-frontend-ts (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Not ready to file as-is. The main defect reports are supported, but several acceptance criteria would introduce incorrect behavior or leave substantial work undefined. Duplication checks below used the exported tracker data, not the live database.

1. **BLOCKER — 06/10 would reject handshake and shutdown.** `SUPPORTED_CAPABILITIES` excludes both commands and includes non-command entries such as `complex_type:*` (`handlers.ts:56–67`). The required “in ALL_COMMANDS but not SUPPORTED_CAPABILITIES → not_supported” rule therefore breaks protocol control; require a separate supported-command set or explicit control-command exceptions.

2. **MAJOR — 01 treats the existing evaluator as a trustworthy oracle for every language.** `triage::evaluate_constraint` uses floating-point division and collapses null/undefined, so a valid Go/Rust integer condition such as `a / 2 == 1` at `a = 3` evaluates differently. An unconditional guard could discard correct constraints; require language/type-aware evaluation or conservatively return indeterminate for unsupported semantics.

3. **MAJOR — 01’s mutation criteria leave common unsound cases uncovered.** Havoc applies only to assignments “inside a loop body,” missing `for` incrementors and mutations in loop conditions; parameter lookup also precedes the flow map (`instrumentor.ts:347`, `:1872`), so updating that map cannot fix reassigned parameters. Add explicit regression cases and requirements for these mutation sites.

4. **MAJOR — 02 permits a branch identity scheme that still collides.** `(line, type)` cannot distinguish two ternaries or two `if` statements on the same line, and permitting different IDs contradicts the unconditional ID-equality tests in 02/09. Require a unique, stable branch identity and same-line regression fixtures.

5. **MAJOR — 02 leaves switch and nullish branch semantics undefined.** Executing a case through fallthrough does not imply `discriminant == case`, while the analyzer currently creates neither default-case branches nor a `??` branch type. Define comparison versus clause-entry decisions, false outcomes, default handling, and nullish analysis/registry changes before requiring exact parity; include fallthrough and effectful case-label tests.

6. **MAJOR — 09 assumes branch metadata that instrument responses do not contain.** `InstrumentResponse` exposes an output path and line count, while internal `InstrumentResult` provides only `branchCount` (`protocol.ts:211–216`, `instrumentor.ts:19–31`). Specify extraction from generated probes, a test interface, or a prerequisite metadata change instead of declaring the fixture issue independent of 02.

7. **MAJOR — 13’s proposed lowering exceeds the core’s supported semantics.** `solver.rs:522` represents null and undefined as integer zero, making the proposed nullish `ite` choose the fallback incorrectly for numeric zero; the solver also explicitly rejects `Shl`/`Shr`. Define sound nullish support or an explicit unknown fallback, and distinguish shift-expression emission from actual solver support.

8. **MAJOR — 07 silently converts an investigation into a substantial implementation.** The exported `str-rf2v` description explicitly excludes consolidation implementation, but this comment requires one shared walker, analyzer data flow, and program-point fixes. Reconcile the issue’s authoritative scope and acceptance criteria—or create a separate implementation ticket—rather than leaving contradictory instructions in its comments.

9. **MAJOR — 08 assumes schema examples are runnable integration fixtures.** Existing requests contain paths such as `src/example.ts`, nonexistent setup files, and fabricated preparation IDs, without frontend applicability or expected handler outcomes. Specify project setup, path/ID substitution, request ordering, and expected results; otherwise the suite can pass while merely confirming incidental `file_not_found` errors.

10. **MAJOR — 09 adds an unbounded cross-frontend audit to a TS fixture ticket.** Requiring a sweep of every frontend’s documentation and Rust tests, followed by filing every discovered workaround, has no predictable completion boundary. Keep this ticket scoped to its known TS workaround and move the broader sweep into separate work.

11. **MAJOR — 14 bundles several independently completable changes.** Request draining, timer cleanup, package-manager policy, build exclusions, standalone bundle repair, and a dependency major upgrade have separate causes and validation requirements. “Independent small commits” does not make them one coherent issue; split lifecycle correctness from the distinct packaging changes.

12. **MAJOR — 14’s lifecycle acceptance allows incomplete fixes.** `unref()` leaves the timer allocated until it fires, so it does not satisfy timer cleanup. Also, shutdown terminates the worker inside `handlers.ts:1058` before the close handler can drain requests; require cancellation of settled timers and a pending-`instrument`-then-shutdown regression that verifies completion before worker teardown.

13. **MAJOR — The mandatory direct E2E commands bypass required setup and governance.** `task e2e-ts` builds both frontends, sets the examples and Go-binary environment, and enters the heavyweight gate wrapper; bare Cargo commands do none of these and conflict with repository resource instructions. Provide an uncached command that preserves that setup and wrapper, with evidence that the intended tests executed.

14. **MINOR — 06’s displayed requests do not reproduce its claimed errors.** Literal `{"command":"execute"}` and `{"command":"analyze"}` fail envelope validation for missing IDs before reaching handlers. Supply complete requests containing `id` and `protocol_version`, or clearly label the examples as abbreviated.

15. **MINOR — 08 overstates the round-trip tests’ uselessness and overlaps tracked work.** JSON round-trip equality does not hold for every plain object containing JavaScript values; exported issue `str-0z1im` already tracks an in-progress negative-zero failure in this suite. State which serialization guarantees must survive the rewrite and explicitly reconcile ownership with that issue.

The three highest-value revisions are to correct the protocol and constraint semantics, split unrelated deliverables and reconcile existing-ticket ownership, and replace assumed test interfaces with executable fixtures and governed verification commands.
