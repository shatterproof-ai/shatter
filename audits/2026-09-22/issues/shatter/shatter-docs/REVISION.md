# Revision: shatter-docs (Codex cross-check, 2026-09-23)

Primary review: `issues/crosscheck/shatter-docs.codex.md`. Secondary: `issues/crosscheck/shatter-docs.md` (degraded same-runtime review). Tracker claims were checked with `bd show` for str-qwua7.24, str-qwua7.25 and str-u394l.3. Code claims were checked in the audit worktree.

## Codex findings

| # | Severity | Finding (one line) | Action | Files changed |
|---|---|---|---|---|
| 1 | MAJOR | #02/#03 artifact-table requirements are incompatible: staged outputs, caller-selected paths, non-JSON formats | applied. #02 now covers the staged-pipeline JSON outputs and publishes an inventory (`protocol/schemas/artifacts/README.md`) that lists each artifact as schema, "no schema: reason" or "not applicable". #03's output column allows default path, "stdout" or "caller-selected path (`<flag>`)". Its schema column is copied from #02's inventory, and a table check has red/green proof. | 02, 03 |
| 2 | MAJOR | #06's equality to INDEX "Users" docs cannot pass | applied. Coverage is now a subset rule: every Users-audience INDEX doc is covered or excluded with a reason, and extra docs are explicitly allowed (INDEX itself, execution-adapters, CI-INTEGRATION, PROTOCOL). Evidence now quotes each doc's real Audience. | 06 |
| 3 | MAJOR | #06: schema-dependent validation can be skipped by cached passes | applied. `protocol/schemas/*.schema.json` and PROTOCOL.md are added to docs-smoke `sources:`, with proof that the task re-executes after a schema-only change. | 06 |
| 4 | MAJOR | #09's reverse check would reject `--max-old-space-size` (SPEC.md:176), so "exactly" is unsound | applied. The reverse check is context-aware: either flag-table/usage-line scope or an `external_flags:` allowlist seeded with `--max-old-space-size`. Proof now requires "at least" the listed drift, with every extra finding fixed or allowlisted with a reason. Verified SPEC.md:176. | 09 |
| 5 | MAJOR | #09's changelog rule allows a header-only bypass | applied. It is split into (a) a new row is required when args.rs or SPEC §2 changes (a header-only edit does not satisfy it) and (b) the header must equal the newest row's date. Same-day rows are allowed. The trailer exemption waives (a) only. Each rule has a red fixture. | 09 |
| 6 | MAJOR | #05 duplicates str-qwua7.25 (frontend line numbers, two `.js` paths) and str-qwua7.24 (TS `ite` claim) | applied. `bd show` confirms both acceptances. The ts :16-17, ts :379-380, rust :234-235 and ts :44 items were removed from #05. The global line-number ban became "lines this issue edits". The refreshed evidence was split into a new note-to-existing on str-qwua7.25 (draft 11). The Go :138 "outcome only" sentence stays in #05 because .24 lists only the TS/Rust files. | 05, 11 (new) |
| 7 | MAJOR | #07's validator misses the `cmds: task:`, `gate-wrapper.sh` and dynamic Affected paths | applied. Validation is restricted to fixed tiers. Expansion must follow `deps`, `- task:` cmds and `bash scripts/gate-wrapper.sh <gate> task <name>`. Unmapped shell commands fail the test, and the Affected row points to `affected-gates.py` instead of listing coverage. Wiring verified in Taskfile.yml (`check` → `check-governed` → stages; `test-standard` → `workspace-test` via cmds). | 07 |
| 8 | MAJOR | #08/#10 overstate the clap-extractor dependency | applied. The recorded str-u394l.3 acceptance (confirmed via bd) needs no CLI extraction, and `check_docs_stories` (drift-patrol.py:453) already checks the index. Both notes now describe the extractor as limiting only automatic CLI-surface completeness. | 08, 10 |
| 9 | MINOR | #06 uses the wrong discriminator: responses use `status`, not `command` | applied. Selection is now by explicit fence tag (`protocol=request`/`response`/`illustrative`), and an untagged fence fails. There is red proof for both a request and a response. Verified at PROTOCOL.md:18-19. | 06 |
| 10 | MINOR | #10's expiry/extension semantics are ambiguous | applied. `pending_since` overrides `created_at`. Age is in whole UTC days: ≤60 is PENDING and ≥61 is FAIL. When the tracker is unavailable, the slot stays PENDING "age unknown" (FAIL under `--strict-pending`). Tests cover 60, 61, the override and unavailable data. | 10 |

## Secondary (same-runtime) findings also applied

| # | Severity | Finding | Action | Files |
|---|---|---|---|---|
| S1 | MAJOR | docs-smoke user-facing contradiction | applied (same fix as Codex 2) | 06 |
| S2 | MINOR | `compute_scan_id_for_targets` is in checkpoint.rs, not scan_orchestrator.rs | applied (verified `shatter-core/src/checkpoint.rs:132`) | 04 |
| S3 | MINOR | evidence paths exist only on branch audit-2026-09-22 | applied: branch named in Source lines and BUNDLE header | 01-07, BUNDLE |
| S4 | MINOR | doctor project-config criterion hedged | applied: run doctor with both config files and paste the output, then document it or correct the str-mktn row | 01 |
| S5 | MINOR | line-number ban widens scope | applied (subsumed by Codex 6) | 05 |
| S6 | MINOR | producer-output schema test has no tier | applied: runs in `task e2e`, with sources listed and direct red/green runs | 02 |
| S7 | MINOR | cross-bucket slugs must be resolved by the filer | no draft change; already listed as cross-bucket blockers in BUNDLE | - |

## Splits / conversions

- **New:** `qwua7-25-refresh-note` (11, note-to-existing str-qwua7.25), split from crate-claude-md-stale-facts. It carries the refreshed evidence for items that .25 already owns.
- No slugs were removed or converted. No `blocked_by` changes within the bucket.

## Decision check

D2 (no Snapshot schema, spec-diff as the regression tool, no `diff` allowlist entry) still holds in 02, 03, 08 and 09. No draft proposes a timeout env var or hook-bypass guidance (D4). Nothing is filed (D6).
