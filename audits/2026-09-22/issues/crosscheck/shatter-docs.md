# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

**DEGRADED review.** The Codex counterpart exited 4 because its output failed identity validation. This is a same-runtime (Claude) fallback review, done read-only against the `audit-2026-09-22` worktree at 56c86168 and the `bd` tracker in /home/ketan/project/shatter.

## Claims checked and confirmed

- `SPEC.md:3` still reads "Last updated: 2026-09-09". Commits 8bd5a667, 21981b1d, c8bceb32 and 2de05fd9 touch SPEC and have no §8 row. The §8 rows for str-1fwt ("2.8, 2.9") and str-mktn ("2.9, 3.6") claim updates the sections do not contain. §3.6 never names `shatter.config.json`. §2.9 doctor text leaves out `-d/--directory` and the gitignore check that `doctor --help` documents.
- `str-qwua7.8` is closed with the reason "Closed". str-wurp has a NOTES block titled "Acceptance checks (to append)".
- `SPEC.md:634` names `--failure-threshold`, and `scan --help` has only `--fail-on-failures [<PERCENT>]` and `--fail-on-setup-error`. `list-targets --help` has `--scope`, and SPEC does not mention it.
- These serde types are at the cited lines: FileSpecBundle, FunctionSpec, ScanReport, ScanSummary, RunStatus, ScanCheckpoint, BehaviorMap, `resolve_artifact_root` and `ScanCheckpoint::default_path` (which hard-codes a 16-hex prefix). No Cargo.toml uses `schemars`. `protocol/schemas/` has 22 frontend-protocol schemas and no artifact schemas. `compare.rs` deserializes `FunctionSpec` directly. shatter-agents `interpret-shatter-spec` SKILL.md:27 cites `shatter-artifacts/<name>.spec.json`.
- Crate CLAUDE.md facts:
  - shatter-core `:7` calls explorer.rs "Concolic".
  - shatter-ts `:16-17` has stale line ranges; the real definitions are `instrumentor.ts:874` and `:1862`. `:44` says Go does not produce `ite`, but the matrix says Go does. str-jeen.40 is closed. The `.js` paths are stale.
  - shatter-go `CLAUDE.md` is 54,640 bytes. No Go source reads `SHATTER_HARNESS_CACHE`. `str-8v66` does not resolve in `bd`.
  - shatter-rust `:109` names `Runtime::new()`, but executor.rs uses `Builder::new_current_thread()` at 2547, 2845, 4942 and 6801. `last_file` is set at `handler.rs:693/767` and read at `:842/:970`. The matrix marks console_output `rust: captured` while its own notes say crate-bridge skips it. `adapters.rs` handles `Multipart`, and the matrix's axum note leaves it out.
- `scripts/docs-smoke.yaml` lists exactly four docs, and the Taskfile `sources:` repeat them. PROTOCOL.md has 22 json fences.
- The `test-standard` deps match the draft. `check-fast` desc says "pre-push". `setup-hooks.sh` selects only `check` or `affected`. `check-fast` appears only at `AGENTS.md:540`.
- `docs/stories/` does not exist. In drift-patrol.py, the PENDING semantics (`:25-32`), the closed-issue FAIL (`:290-307`) and the slot definitions (~`:437`, ~`:453`) match. The open dates of str-wurp (2026-06-12) and str-u394l.3 (2026-06-17) match.

## Findings

1. **MAJOR — docs-smoke-coverage: the acceptance criteria contradict each other on which docs count as user-facing.** AC1 adds `docs/execution-adapters.md` and `docs/CI-INTEGRATION.md` to docs-smoke. AC3 requires the docs-smoke list to equal the `docs/INDEX.md` docs "whose Audience includes users", minus exclusions. INDEX lists CI-INTEGRATION's audience as "Contributors" and execution-adapters' as "Contributors and architects", so neither includes users. Following both criteria literally makes the new test fail on the committed list. The Problem section also calls these docs "user-facing". Fix: define the coverage set as an explicit inclusion list plus a rule such as "every Current-behavior doc in INDEX's Primary/Supplementary tables", or drop the two contributor docs from AC1.
2. **MINOR — spec-s6-layout-and-checkpoint: wrong file for the scan-id function.** `compute_scan_id_for_targets` and the `scan_id_v2:` prefix are in `shatter-core/src/checkpoint.rs:132`, not `scan_orchestrator.rs`. Correct the pointer.
3. **MINOR — bundle-wide: evidence paths exist only on the unmerged audit branch.** `audits/2026-09-22/areas/*.md` and `audits/2026-09-22/artifact-samples/*` exist only on branch `audit-2026-09-22`. A fresh agent on `main` cannot open them. Most evidence is restated inline, so this is not blocking. Still, either land the audit directory first or name the branch in each Source line.
4. **MINOR — spec-changelog-backfill: the doctor "project-configuration report" criterion is hedged.** The str-mktn §8 row says doctor reports whether both config files are present and their precedence, but `doctor --help` does not mention this. The criterion "(if doctor prints one)" leaves it open. Require the implementer to check the behaviour, then either document it or correct the str-mktn row.
5. **MINOR — crate-claude-md-stale-facts: the line-number ban widens the scope.** "No crate CLAUDE.md cites a source line number" applies across all four files, including shatter-go's 54 KB file, not only the lines listed under Evidence. That overlaps with str-qwua7.25's slimming. It is acceptable, but estimate it as more than a cheap fix, or limit it to the cited items plus the new lint.
6. **MINOR — artifact-json-schemas: the producer-output validation test has no gate tier.** The test runs `explore --spec-out` and a `scan` on examples. That needs built frontends, which is too heavy for check-static. State which tier runs it (for example parity or e2e) and list its sources, the same way the `--check` criterion already does.
7. **MINOR — cross-bucket slugs.** spec-s5 (blocked by `spec-json-shapes-compare`), spec-s6 (`mixed-language-scan-deletes-artifacts`) and test-tier (`collapse-test-tiers`, `ci-executed-leaf-guard`) depend on drafts in other buckets. The filer script must resolve these to `str-*` IDs, or the dependencies will be dangling text.

Checks that found no problems:
- Draft 01 correctly leaves D2 §2.6/§2.11/§5.5 removal to retire-snapshot-diff.
- Draft 02 correctly excludes the Snapshot schema (D2).
- Drafts 08, 09 and 10 are notes on correct, open target issues and change no status.
- No duplicates were found beyond the relationships the drafts already name.

## Verdict

Ready to file after one fix. The factual claims re-verify well, with only one wrong file pointer. Top fixes:
1. Resolve the "user-facing" contradiction in docs-smoke-coverage (finding 1).
2. Fix the `compute_scan_id_for_targets` location in spec-s6 (finding 2).
3. Make sure the filer maps the cross-bucket slugs to real IDs, and that the `audits/2026-09-22/` evidence is reachable from `main` (findings 3 and 7).
