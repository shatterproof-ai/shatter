# Cross-bucket effects reported by revisers (2026-09-23)

## bugshot-tracker-and-payload
- removed/converted: none
- new: publish-staged-plugin-bundle
- New bugshot slug publish-staged-plugin-bundle (06) is blocked_by installed-cache-bloat-investigation (03). It owns the bugshot payload/publication fix. If the chosen route needs it, 06 must file a linked bento issue to change the bugshot marketplace `source` entry. A bento-bucket draft on the bugshot marketplace source would duplicate that and should reference 06 instead.

## shatter-docs
- removed/converted: none
- new: qwua7-25-refresh-note
- crate-claude-md-stale-facts no longer covers the items str-qwua7.25 owns (frontend CLAUDE.md line numbers and the two .js paths) or the TS ite claim that str-qwua7.24 owns. A new note-to-existing on str-qwua7.25 (qwua7-25-refresh-note, draft 11) carries that evidence, finding docs-24 included. Any other bucket that expected crate-claude-md-stale-facts to absorb docs-24 should point to qwua7-25-refresh-note instead.
- artifact-json-schemas now also covers the staged-pipeline JSON outputs (observe/analyze/solve) and publishes protocol/schemas/artifacts/README.md as the inventory that spec-s5-contract-table-and-samples copies. spec-json-shapes-compare is unaffected.
- The docs-smoke header-date check (spec-changelog-backfill) and str-wurp's rule (b) are the same check. The str-wurp note says to reuse it, not duplicate it.
- Cross-bucket blocked_by slugs are unchanged. The filer still has to resolve spec-json-shapes-compare and mixed-language-scan-deletes-artifacts to str IDs.

## shatter-agents/shatter-agents-plugin
- removed/converted: none
- new: skill-status-metadata, sa-d8j-engine-discovery-note
- Engine gap for the shatter tracker (not filed in this bucket): `shatter list-targets` selects sources under .shatter/cache/harness/ (reproduced on a temp tree at shatter 70465921; GLOB_WALK_EXCLUDE_DIRS excludes .shatter but list-targets discovery does not). Candidate for the shatter-cli-flags-and-help or artifacts bucket.
- Engine gap for the shatter tracker: no shatter command rejects a malformed .shatter/config.yaml (list-targets exits 0 on `foo: [unclosed`), and `shatter doctor` does not parse it.
- Drafts 01, 02, 05 reference shatter slug retire-snapshot-diff (bucket shatter-artifacts-correctness), and 05 and 08 reference release-publish-and-install-smoke (bucket shatter-ci-workflows), as `<slug id>` placeholders the filer must substitute. Those slugs must stay stable in their buckets.
- delegate-discovery-to-engine slug kept, but its scope narrowed to shatter-doctor only. Its run_targets.py/list-targets half moved to the new note-to-existing on sa-d8j.

## dotfiles-global-guidance
- removed/converted: none
- new: first-party-plugin-staleness-check, codex-render-composes-core-rules
- MANIFEST.md rows 419-429 say 'File other/NN as is'. They are out of date: 04 now covers autoUpdate only (the checker moved to new slug first-party-plugin-staleness-check, 12-first-party-plugin-staleness-check.md), and 01 now covers Claude delivery only (Codex delivery moved to new slug codex-render-composes-core-rules, 13-codex-render-composes-core-rules.md, blocked_by global-guidance-actually-loads). The MANIFEST and INDEX need rows for both new slugs.
- never-recommend-bypass's Codex BLOCKER (D4 contradiction) is resolved. It is not in file-all.sh BLOCKER_SLUGS, so no hold change is needed.
- Slug references to other buckets are unchanged and still required: bento check-unpushed-overcount-and-blocks, bento git-guard-bypasses-and-false-positives, shatter-agents withdraw-shatter-diff-skill.
- Shatter memory (outside any bucket): project_shatter_gate_cache_and_bare_primary.md and its MEMORY.md index line still claim core.bare=true (the actual value is false). project_audit_2026_07_10_gate_state.md is still unindexed. memory-lifecycle-rule now makes correcting both part of its close; the audit report's claim that the memory was 'corrected 2026-09-23' is only partly true.
- file-all.sh DRY_RUN ONLY=dotfiles printed no errors from this bucket. The 4 errors it did print are unknown blocked_by slugs in bento/bento-guards-doctor-tracker (01, 04) and shatter/shatter-concolic-and-engine-design (01).

## shatter-artifacts-correctness
- removed/converted: none
- new: explore-spec-bundle-failed-functions
- spec-json-shapes-compare (shatter-reports-and-specs) now owns compare support for bundles and multi-file inputs; multi-file-spec-bundle-first-only dropped compare from its acceptance. The shared spec reader must accept the multi-file shape that multi-file-spec-bundle-first-only chooses, so relate or order the two.
- artifact-json-schemas and spec-s5-contract-table-and-samples (shatter-docs) should also describe failed_functions and the new Failed bundle status from the new slug explore-spec-bundle-failed-functions.
- str-qwua7.11 (existing, open P1) now owns the fresh-directory whole-stdout-JSON test for explore --spec-json. That case was removed from the str-qwua7.39 note (qwua7-39-json-stdout-first-run).

## shatter-cli-runtime-output
- removed/converted: none
- new: doctor-rust-runtime-note, rust-hint-once-note, doctor-execution-readiness, rust-main-default-exclusion, rust-mocks-to-string-diagnosis, run-validity-degraded-cause-diagnosis, coverage-headline-metric-unification
- issues/INDEX.md and issues/MANIFEST.md need the 7 new slugs (09-15) of this bucket; no slug was removed or converted.
- markdown-drops-render-plain-info (referenced by shatter-cli-flags-and-help/01 explore-format-flag-ignored) no longer claims markdown completeness or plans plain removal; it now delivers a plain-vs-markdown parity inventory that explore-format-flag-ignored should use before removing --render plain.
- Coverage-metric unification moved out of run-report-verdict-and-coverage-metrics into new coverage-headline-metric-unification (picks line coverage as headline); it is related to, not blocked by, branch-metric-counts-sites (shatter-reports-and-specs).
- Doctor Rust-section work routed to existing str-qwua7.40 (note 09) and hint dedup to str-qwua7.13 (note 10); the two existing issues disagree on require-flag spelling (--require-rust vs --require-frontend <lang>), flagged in notes 09 and 11.
- scan-progress-post-hoc now defines the machine-mode (--progress) stderr JSON contract, including warn/error log objects, which sandbox-backend-disables-guard and rust-runtime-path-and-doctor reference.

## bento-landing
- removed/converted: landing-deletes-remote-branches
- new: land-py-branch-flag, stale-pushed-branch-doctor-nudge, one-session-per-branch-guidance
- landing-deletes-remote-branches is converted from a new issue to a note-to-existing on bento-73de. The slug is retained, so it is not deleted. file-all.sh BLOCKER_SLUGS holds this slug with the reason 'duplicates open bento-73de; convert to a note'. The conversion is done, so the maintainer can lift that hold.
- merge-message-and-stale-branch-nudge now covers only the merge message (slug kept). The stale-branch doctor nudge moved to the new slug stale-pushed-branch-doctor-nudge, and the one-session rule to the new slug one-session-per-branch-guidance. The bare-SHA merge guard was dropped because bento-rdtn.15 already blocks every Bash git merge in the primary checkout. Remaining guard gaps are pointed at bento-i76i and bento-guards-doctor-tracker's git-guard-bypasses-and-false-positives.
- The new drafts cite bento-guards-doctor-tracker slugs in backticks: claim-branch-reconciliation (in 16 and 17) and close-reason-evidence (in 07 and 14). Those slugs must keep their names.
- land-py-verifier-log-kept (02) now carries a companion '## Comment for bento-x4bm' that fixes the shared verifier-log location as <git-common-dir>/bento/landing/<key>/. land-py-invocation-progress-log (06) now writes its progress log into that same directory; it previously used an XDG state dir.
- land-work-skill-restructure (09) should land after bento-49pg. Its Tracker Handoff criterion carries 49pg's single Option C rule, which is consistent with D4 and with bento-guards-doctor-tracker's beads-dolt-remote-guidance.

## shatter-agent-guidance-and-repo-hygiene
- removed/converted: none
- new: qwua7-14-reverify-on-main, orphan-worktree-dirs-cleanup, u394l-4-skill-command-lint, qwua7-52-storystore-interim-nudge
- shatter-tracker-and-beads/08-tracker-reconciliation-sweep.md:91 names fixture-corruption-incident-reverify for re-verifying str-qwua7.14; that work now lives in the new reopen-note qwua7-14-reverify-on-main (existing_id str-qwua7.14). The filer only posts the comment, so `bd reopen` stays a manual step.
- env-doctor-decisions changed kind from new to note-to-existing (existing_id str-qwua7.53), slug kept; MANIFEST.md rows 324/507/657/854 still describe it as a new issue. Its storystore half is now the note qwua7-52-storystore-interim-nudge on str-qwua7.52, and its orphan-dir removal is now the new issue orphan-worktree-dirs-cleanup.
- planning-rules-location-and-open-decisions no longer comments on str-qwua7.43; shatter-concolic-and-engine-design/qwua7-43-bench-dev-dep-cycle is the only owner of that scope addition. Its reference to planning-rules-location-and-open-decisions still holds.
- repo-skills-rot keeps blocked_by beads-retire-jsonl-import-dolt-remote (bucket shatter-tracker-and-beads), for its AC6 only; the skill-command lint moved to the note u394l-4-skill-command-lint on str-u394l.4.

## shatter-concolic-and-engine-design
- removed/converted: none
- new: explore-stop-reason-accounting, concolic-early-termination-fix, explore-budget-semantics, concolic-fuzz-rng-unseeded, concolic-benchmark-postfix-run, holdout-disposition, effectiveness-repo-tracker-backlog, underscore-binding-lint, pipeline-close-reason-rule, core-reachability-gate, qwua7-6-2-scan-observe-config-literals
- concolic-early-termination is now diagnosis-only; references meaning 'the fix' should point to concolic-early-termination-fix (new, blocked by concolic-early-termination).
- shatter-engine-correctness/07-duplicate-value-shrinkers cites core-dead-code-removal for the crate-wide dead-code check; that check is now core-reachability-gate.
- shatter-frontend-ts/09-ts-branchtype-known-answer-fixtures and 02-ts-switch-ternary-instrumentation cite engine-parity-e2e for the _-param lint and checklist rule; those are now underscore-binding-lint and pipeline-close-reason-rule.
- shatter-engine-correctness/02-float-probe-paths-uncounted cites engine-path-identity-budget-config for path identity: still correct (slug now means path identity only; budget semantics moved to explore-budget-semantics).
- shatter-cli-flags-and-help/05-seed-for-explore-and-run: concolic-vs-default-benchmark now uses scan --seed and is blocked by concolic-fuzz-rng-unseeded; it needs seed-for-explore-and-run only if the harness uses explore.
- New note-to-existing target str-qwua7.6.2 (qwua7-6-2-scan-observe-config-literals) for the filer.

## shatter-ci-workflows
- removed/converted: none
- new: release-publish-guard-and-target, devcontainer-workflow-red, docker-publish-workflow-red, go-lint-qwua7-32-note, ubuntu-26-runner-trial, perf-ci-stable-scenarios-red, parity-governed-stale-fallback
- bento-landing/04-land-work-post-push-workflow-health.md says automatic filing for red workflows 'belongs to the shatter-side workflow-health-patrol'. workflow-health-patrol is read-only and files nothing (its devcontainer and docker-publish follow-ups are now pre-drafted as devcontainer-workflow-red and docker-publish-workflow-red), so that sentence should be corrected.
- shatter-tracker-and-beads (D4): new draft devcontainer-workflow-red is one more .beads/issues.jsonl consumer (.devcontainer/post-create.sh runs bd init --from-jsonl + bd import). beads-jsonl-consumers-drop-bd-sync / beads-retire-jsonl-import-dolt-remote should list it as a consumer. It is linked as a relation, not blocked_by.
- shatter-gates-integrity/02-ci-executed-leaf-guard: its reference to nextest-ci-profile-and-stale-parity-fallback is still valid (slug kept). The parity-governed fallback now lives in parity-governed-stale-fallback.
- shatter-agents drafts citing release-publish-and-install-smoke are unaffected (slug kept). That issue is now also blocked by release-publish-guard-and-target.

## shatter-cli-flags-and-help
- removed/converted: none
- new: analyze-only-sandbox-refusal, analyze-only-output-detail, explore-function-not-found-diagnostics, spec-flag-dropped-with-spec-out, html-source-non-executable-lines, failure-table-language-any, demo-complete-with-errors-green, exit-codes-qwua7-12-note, unfiled-0904-ui-items-reconcile
- seed-for-explore-and-run (this bucket) now has blocked_by: [explore-resume-options-key], a slug in shatter-artifacts-correctness/01. The filer must resolve that cross-bucket edge.
- New note-to-existing exit-codes-qwua7-12-note targets open str-qwua7.12 (P1). It replaces the earlier plan to close str-qwua7.12, because the live acceptance criteria require exit 1 on multi-target partial failure but explore.rs:7075 pins exit 0. Any other bucket that assumes str-qwua7.12 is done should not.
- explore-format-flag-ignored no longer retires --render. Global --render retirement and scan's migration are deferred to existing str-9ee5 (open). markdown-drops-render-plain-info (shatter-cli-runtime-output) is unaffected.
- help-hides-execution-flags now keeps --timing* global and scopes --set to explore only. Anything elsewhere that assumes an ExecOptions struct carrying --set/--timing for all executing commands should be adjusted.

## bento-guards-doctor-tracker
- removed/converted: closure-orphan-worktree-dirs
- new: l01v-residual-bypasses-note, i76i-switch-update-ref-note, 49pg-dolt-remote-section-note, wzbt-manual-close-note
- closure-orphan-worktree-dirs (09) is now a note-to-existing on bento-nljv (same slug). No other bucket references it.
- git-hook-latency-visibility is now blocked_by land-py-invocation-progress-log (bucket bento-landing), so that slug must stay stable.
- close-reason-evidence now covers only non-landing closes (not reproducible / duplicate / superseded / wontfix) and is blocked by bento-wzbt. Closure reasons after a landing belong to bento-wzbt/bento-x4bm. shatter-concolic-and-engine-design/06-engine-parity-e2e cites close-reason-evidence as the generic bento close-reason rule; it should also link bento-wzbt for landing closes.
- git-guard-bypasses-and-false-positives no longer covers false positives or the rtk/command/env wrappers (bento-l01v), or switch/update-ref (moved to a note on bento-i76i). dotfiles-global-guidance/03-never-recommend-bypass still correctly points to it for the /usr/bin/git and GIT_CONFIG_* bypass forms.
- check-unpushed-overcount-and-blocks: on the primary branch in the primary checkout the check is now always report-only, and land.py suppression is per branch, not per session. The dotfiles 02-background-wait-rule-and-hook and bento-landing 12-rebase-before-land-configurable references stay valid.
- beads-dolt-remote-guidance no longer adds a missing-Dolt-remote doctor warning, because linked worktrees share one DB. It is blocked by bento-49pg. The references from bento-landing/09 and the shatter tracker-and-beads drafts stay valid.
- bento-cross-check-bug/BUNDLE.md contains a copy of cross-check-stop-hook-hijack but no NN file, so only this bucket's 15 is filed. That bundle was not changed.
- New comments will go to bento-l01v, bento-i76i, bento-49pg and bento-wzbt (note drafts 16-19), plus a companion comment on bento-x4bm from followups-as-siblings.

## storystore/storystore-adoption-blockers
- removed/converted: none
- new: tracker-migration-verification, clap-builder-extractor, go-cobra-extractor, coverage-unextracted-language-finding
- clap-cobra-extractors keeps its slug. Its scope is now Rust clap derive only (top-level and nested), plus per-kind language metadata and the spec.md contract update. The shatter-docs 08/10 references (the issue that makes shatter's clap subcommands visible) stay accurate. Go cobra, the clap builder API and the coverage-gap finding moved to go-cobra-extractor, clap-builder-extractor and coverage-unextracted-language-finding.
- tracker-migration-and-agents-md keeps its slug but now covers only AGENTS.md/CLAUDE.md and the plan-doc move. The migration record, other-clone bootstrap and beads.role moved to the new tracker-migration-verification. The migration itself is still a filer precondition for the whole bucket.
- MANIFEST.md/INDEX.md rows for this bucket (lines ~396-399) are stale: 4 new slugs, changed titles, and clap-cobra-extractors/automatic-version-bump no longer have blocked_by tracker-migration-and-agents-md. The new filing order is 05,01,04,02 -> 06,07,08 -> 03.

## shatter-test-hygiene
- removed/converted: none
- new: cli-output-snapshots, e2e-once-in-pre-completion, ts-handlers-timeout-fix
- The E2E double-run in pre-completion-e2e moved from collapse-test-tiers to the new unblocked slug e2e-once-in-pre-completion. Two drafts still name collapse-test-tiers as its owner: shatter-docs/07-test-tier-docs-overstate-coverage.md:51 and shatter-gates-integrity/04-affected-gates-routing.md:55.
- ts-handlers-test-timeouts is now diagnosis only; the fix is the new slug ts-handlers-timeout-fix, which ts-handlers-test-timeouts blocks. shatter-gates-integrity/02-ci-executed-leaf-guard.md:29 references ts-handlers-test-timeouts, and that reference still resolves.
- CLI output snapshots were split out of snapshot-test-helpers into cli-output-snapshots. The shatter-reports-and-specs/08 and 09 references to snapshot-test-helpers and pin-examples-repo remain valid.

## shatter-engine-correctness
- removed/converted: z3-mixed-int-real-sort-split, float-constant-rational-conversion
- new: t854z-sort-split-note, aureo-float-constant-note, concolic-refine-path-accounting, execute-request-builder
- shatter-concolic-and-engine-design: replace slug z3-mixed-int-real-sort-split with existing id str-t854z in 02-concolic-early-termination.md (:69, :81, :102), 12-concolic-early-termination-fix.md (:42, :53) and BUNDLE.md (:193); the draft was converted to note-to-existing on str-t854z (slug t854z-sort-split-note)
- float-constant-rational-conversion converted to note-to-existing on str-aureo (slug aureo-float-constant-note), proposing str-aureo P2->P1; no other bucket references it
- concolic-refine-execute-builder keeps its slug but now covers only the refine prepare_id/execution_profile fix; the shared builder is new slug execute-request-builder and refine path accounting is new slug concolic-refine-path-accounting. The reference in shatter-concolic-and-engine-design/06-engine-parity-e2e.md (refine drops prepare_id) stays correct
- execute-request-builder should also be blocked by existing open str-qwua7.5 at filing time (blocked_by holds only in-bundle slugs)

## shatter-protocol-parity
- removed/converted: none
- new: parity-matrix-unenforced-sections, conformance-success-case-per-command, conformance-known-drifts-matching, conformance-cross-frontend-execute-cases
- MANIFEST.md mappings: protocol-parity-02, prior-05 and gates-10 (-> conformance-harness-correctness) now also cover the split slugs conformance-known-drifts-matching and conformance-cross-frontend-execute-cases; protocol-parity-01 (-> parity-dispatch-reconciliation) also maps to conformance-success-case-per-command; protocol-parity-08 (-> capability-single-source) also maps to parity-matrix-unenforced-sections.
- Drafts 01, 03, 06, 17 now reference task-sources-cover-real-inputs (shatter-gates-integrity) as the owner of the existing parity.sources omission (matrix, PARITY.md, validate-parity.py); they add the input themselves only if that issue has not landed. There is no blocked_by edge.
- Ownership of the frontend-parity skill and crate CLAUDE.md capability tables, including the ite, analyze-stub and Rust-timeout prose, is left explicitly with the existing str-qwua7.24 (OPEN). capability-single-source and parity-guidance-skill-and-template no longer claim it.
- The known_drifts dangling divergence ID is left with the existing str-qwua7.34 (OPEN). conformance-known-drifts-matching does not duplicate it.

## shatter-gates-integrity
- removed/converted: none
- new: qwua7-55-verifier-timeout-note, qwua7-2-scope-note, ci-first-real-run-triage, sccache-for-gate-runs
- ci-executed-leaf-guard no longer owns triage of newly surfacing CI failures or the CLAUDE.md CI-claim fix. Triage moved to new slug ci-first-real-run-triage (blocked by task-list-json-poisons-checksums and ci-executed-leaf-guard). The CLAUDE.md CI claims are left to shatter-docs/test-tier-docs-overstate-coverage, which should restore the stronger claim on ci-first-real-run-triage's green run, not on ci-executed-leaf-guard landing.
- shatter-ci-workflows/12-ci-runs-user-paths and 14-nextest-ci-profile-and-stale-parity-fallback reference ci-executed-leaf-guard for CI proof. The slug is unchanged, but a fully green CI run is now ci-first-real-run-triage's close proof. ci-executed-leaf-guard proves only that the leaves executed.
- verifier-per-language-evidence (slug kept) is rescoped to /pre-completion rows only. Verifier timeout, output and executed-reporting items are now note-to-existing drafts on str-qwua7.55 (qwua7-55-verifier-timeout-note) and str-qwua7.2 (qwua7-2-scope-note). Other buckets citing verifier-per-language-evidence for verifier changes should point to those notes.
- gate-telemetry-executed-vs-cached no longer covers sccache (new slug sccache-for-gate-runs, P3) or the cross-project slot (out of scope, bento-dyp7). It is now also blocked by ci-executed-leaf-guard.
- ci-executed-leaf-guard now defines the shared per-leaf evidence parser (e.g. scripts/task_leaf_evidence.py) reused by verifier-per-language-evidence, gate-telemetry-executed-vs-cached and the str-qwua7.2 note. Other buckets needing an 'is up to date' parser should reuse it.
- fast-hermetic-precommit dropped pre-push receipt reuse (owned by str-35vtk.25/.26/.9) and scopes governance to pre-commit only. It still contains no hook-bypass guidance and no timeout env var (D4).

## shatter-frontend-rust
- removed/converted: none
- new: qwua7-36-escaping-repro, rust-build-deadline-enforcement, rust-input-deserialize-classification, rust-axum-extractor-classifier-dedupe, shatter-llm-parse-validation, shatter-llm-backoff-cap, llm-model-rejection-diagnostic
- INDEX.md / MANIFEST.md rows for shatter-frontend-rust are stale: 19 drafts now (was 12), titles changed for rust-instrument-constraints, rust-usize-negative-inputs, rust-usize-reopen-note, rust-tests-offline-silent-pass, rust-frontend-design-dedupe, rust-runtime-harness-loop, shatter-llm-hardening; regenerate at index step
- New note-to-existing qwua7-36-escaping-repro targets str-qwua7.36; rust-instrument-constraints is now blocked by it (filer resolves to str-qwua7.36)
- rust-usize-negative-inputs re-scoped to the int_range() 64/128-bit unsigned lower bound in shatter-core/src/types.rs; any engine-correctness/input-gen bucket touching int_range or unsigned bounds should reference it
- shatter-llm-hardening narrowed to API-key redaction; parser validation and backoff moved to shatter-llm-parse-validation and shatter-llm-backoff-cap
- rust-tests-offline-silent-pass no longer relies on str-jyxr for fixture prefetch (str-jyxr is the input-prefetch generate budget); test-hygiene/CI buckets citing str-jyxr as cargo prefetch should be corrected
- rust-crate-bridge-stdout now carries a D1 Windows requirement (release matrix ships shatter-rust.exe); release/CI buckets may want to note that the standalone/dispatch harness generators are Unix-only

## shatter-frontend-ts
- removed/converted: none
- new: ts-flow-analysis-consolidation, core-constraint-consistency-guard, ts-packaging-hygiene, ts-js-yaml-v4
- ts-branchtype-known-answer-fixtures no longer includes the repo-wide sweep for prose test workarounds. Its Out of scope now points that sweep at pipeline-close-reason-rule (shatter-concolic-and-engine-design) or a separate bounded issue. That bucket may want to take it on.
- The core-side constraint consistency guard moved out of ts-flow-map-program-point into the new core-constraint-consistency-guard (P2, core). concolic-early-termination and its fix draft can link the new slug as a diagnostic aid, since its inconsistent_constraints counter helps attribute early stops.
- ts-lifecycle-and-packaging-hygiene (slug kept) is now lifecycle only. Packaging moved to ts-packaging-hygiene and the js-yaml upgrade to ts-js-yaml-v4. A grep found no other bucket that references these slugs.
- Every E2E close-time proof in this bucket now uses `task --force e2e-ts/e2e-go/e2e-rust` instead of bare `cargo test --test e2e_*`, per AGENTS.md. Other buckets that still require bare cargo E2E commands have the same defect that Codex finding 13 raised here.

## shatter-tracker-and-beads
- removed/converted: none
- new: beads-hook-stall-diagnosis, audit-2026-09-04-report-recovery, qwua7-22-audit-land-before-file-note, audit-branch-deletion-cause, qwua7-12-rescope-note, landed-not-closed-patrol-check, triage-drift-patrol-checks
- shatter-cli-flags-and-help cli-minor-output-and-help-polish item 11 still proposes closing str-qwua7.12. Live `bd show` shows its multi-target 'exit 1 on partial failure' acceptance check is unmet: explore.rs:7075 asserts exit 0. Change item 11 to comment-only and point it at the new qwua7-12-rescope-note. help-tracker-ids-lint references a nonexistent slug exit-codes-qwua7-12-note, which qwua7-12-rescope-note can replace.
- tracker-reconciliation-sweep no longer has blocked_by qwua7-1-git-state-check. The filer would have resolved that slug to str-qwua7.1 and blocked the sweep for good. The ordering is now stated in prose only.
- publish-audit-reports no longer rewrites /audit SKILL.md. That rewrite stays with str-qwua7.22, via the new note qwua7-22-audit-land-before-file-note. shatter-agent-guidance-and-repo-hygiene/06-repo-skills-rot.md:116 says the audit skill belongs to publish-audit-reports and should point to str-qwua7.22 instead.
- Filing bootstrap: the maintainer creates the recovery refs, then files only the shatter epic and publish-audit-reports, lands that issue, and only then runs the bulk filer. file-all.sh can do this today with ONLY=shatter and a HOLD list of every other shatter slug. An include-list option would be simpler.
- beads-jsonl-consumers-drop-bd-sync now owns every line-level `bd sync` removal in AGENTS.md, including :125 and :345-369. This matches the existing qwua7-23-agents-md-rtk-and-landing note. str-qwua7.23 keeps only the rewrite of the landing sections.
- beads-retire-jsonl-import-dolt-remote is now also blocked by the new beads-hook-stall-diagnosis. Its latency targets (<15 s worktree add, <30 s create_preview) now depend on what the diagnosis attributes the wait to.

## shatter-frontend-go
- removed/converted: none
- new: go-release-relocation-smoke, go-concolic-escaped-string-miss, go-connection-failures-impl, go-line-zero-records, go-mock-codegen-json
- go-build-timeout-ignored (09) is now blocked_by `timeout-budget-invariant` (shatter-frontend-rust/03-timeout-budget-invariant.md). That slug must stay stable, and its request>build+exec invariant must cover Go execute requests that build, not only Rust.
- New go-release-relocation-smoke (14) is blocked_by `release-publish-and-install-smoke` (shatter-ci-workflows/03). That draft's smoke job runs explore 'from a checkout' on a runner that uses the same path as the build job, so it would NOT catch the Go harness relocation bug. 14 adds the steps that do (different checkout path, shatter-go/ deleted, fresh SHATTER_GO_WORKSPACE_ROOT) to that job. The slug must stay stable.
- INDEX.md/MANIFEST.md counts for shatter-frontend-go are now stale: 18 drafts instead of 13 (16 new, 1 note-to-existing, 1 reopen-note). The INDEX cross-check row should point at the Codex review and REVISION.md.
- No other bucket references any slug from this bucket (checked by grep), and no slug was removed or converted.

## shatter-reports-and-specs
- removed/converted: none
- new: spec-diff-symbolic-region-verdicts, ts-union-discriminant-literals, qwua7-10-allowlist-issue-links-note, source-bucket-config-override
- shatter-concolic-and-engine-design 02-concolic-early-termination (lines 71, 81, 102) and 12-concolic-early-termination-fix (line 42) cite known-answer-ratchet-and-ts-discriminants for the computeArea discriminant widening; that work moved to the new slug ts-union-discriminant-literals, so retarget those references (the old slug now covers only the known-answer ratchet gate).
- shatter-test-hygiene pin-examples-repo now blocks golden-and-consumer-suite and known-answer-ratchet-and-ts-discriminants.
- shatter-gates-integrity gauntlet-scan-checker-consumes-json still blocks golden-and-consumer-suite (unchanged); golden-and-consumer-suite is now also blocked by spec-json-shapes-compare and scan-report-headline-and-paths.
- New note-to-existing on str-qwua7.10 (qwua7-10-allowlist-issue-links-note): if another bucket also notes str-qwua7.10, the filer should merge the notes.
- spec-json-shapes-compare moves spec-diff's SpecInput reader out of shatter-cli/src/commands/diff.rs into core; shatter-artifacts-correctness retire-snapshot-diff (D2) touches the same file, so whichever lands second rebases.
- shatter-docs spec-s5-contract-table-and-samples is still blocked by spec-json-shapes-compare; the SPEC section 5 table should describe the reader's data contract (a JSON bundle or bare spec; YAML is output-only).
