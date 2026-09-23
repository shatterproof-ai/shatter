# Audit 2026-09-22: issue reconciliation manifest

Global reconciliation of the five draft sets under `audits/2026-09-22/drafts/`, the verified findings in `findings.json`, and the report (`audits/2026-09-22.md`, sections 14, 15, 15.1 and Coverage gaps). It applies the maintainer decisions of 2026-09-23 (D1-D6). Nothing has been filed. Under D6 the maintainer runs one filer script after this manifest and the Codex cross-check.

## Maintainer decisions applied

| ID | Decision | Effect on the drafts |
|---|---|---|
| D1 | Keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix | code/70 split into release-windows-z3-build, release-aarch64-openssl-cross and release-publish-and-install-smoke. The 'drop from the matrix' options are removed. Each closes only with a green release-run URL. |
| D2 | Retire snapshot `shatter diff`; spec-diff is the regression tool | code/38 becomes retire-snapshot-diff, which also updates SPEC, README and QUICKSTART. A note tells str-81xiw that the `diff` name is free. other/21 now withdraws the shatter-diff skill and points readers to spec-diff. |
| D3 | Measure concolic first | concolic-vs-default-benchmark (P1) and concolic-early-termination (P1) are filed, plus concolic-positioning-decision, which both block. No doc softening. |
| D4 | Retire the beads JSONL import and sync through a Dolt remote | agent/05 and agent/08 are replaced by three shatter issues: clobber check, then retire the import and add a Dolt remote, then fix consumers and drop `bd sync`. str-qwua7.28, str-ly5bz and str-mpgg1 get close notes. bento/17 is rewritten as the Dolt-remote guidance and bento/03 loses the env-var suppression. No hook-timeout env var and no bypass guidance anywhere. |
| D5 | `[user]` section already removed; add `.mailmap`, a git-state check and a fixture config snapshot | agent/04 becomes mailmap-and-fixture-config-snapshot. The check is added to str-qwua7.1 as a note, and str-qwua7.51 gets a root-cause note. |
| D6 | One filer script, run by the maintainer | No per-set file.sh is run, and no agent files anything. |

## Epics (exactly one per target tracker)

| Repo | Epic title | Tracker |
|---|---|---|
| shatter | Epic: Audit 2026-09-22 findings | bd in /home/ketan/project/shatter (prefix str) |
| bento | Epic: Audit 2026-09-22 findings (bento) | bd in /home/ketan/project/bento (prefix bento) |
| shatter-agents | Epic: Audit 2026-09-22 findings (shatter-agents plugin) | bd in /home/ketan/project/shatter-agents (prefix sa; config has no issue-prefix, prefix taken from existing ids such as sa-tyb) |
| storystore | Epic: Audit 2026-09-22 findings (storystore) | bd in /home/ketan/project/storystore (prefix ss; bd writes are blocked until the maintainer runs the pending v32->v53 schema migration, so file this epic after the migration) |
| bugshot | Epic: Audit 2026-09-22 findings (bugshot) | bd in /home/ketan/project/bugshot (prefix bgs) |
| dotfiles | Epic: Audit 2026-09-22 findings (global agent guidance and hooks) | gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo) |

Totals: 22 buckets; 187 new issues, 32 notes to existing issues, 25 reopen-notes (comments on closed issues that point to the new issue).

Bucket sizes: every bucket is single-repo. `storystore-adoption-blockers` (4) and `bugshot-tracker-and-payload` (5) are below the 8-issue floor because those repos have no other findings.

## Buckets

### shatter-gates-integrity (shatter, 10 entries)

Gates must prove they executed: Task checksum poisoning, sources, affected routing, pre-commit, verifier evidence, dead gauntlet checker, unwired test modules.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| task-list-json-poisons-checksums | note-to-existing | str-qwua7.3 | P1 | `code/01`, gates-01, tests-ci-01 | - | Post as a comment on open str-qwua7.3, not as a new issue: this finding is the root cause .3 asks for. Widen .3's acceptance to the fix (no `task --list-all --json` / use --dry or an isolated TASK_TEMP_DIR in the meta stage), a regression test proving stage-2/3 leaves execute after a meta run, and a real re-run of main. Use code/01's body as the comment text. |
| ci-executed-leaf-guard | new | - | P1 | `code/02`, gates-02 | - | File code/02 as is (CI guard that fails when a test leaf reports 'up to date', plus triage of what fails once tests really run). Blocked by the str-qwua7.3 fix. Link str-qwua7.2 (receipts) and str-35vtk.21 in the body; do not file a separate receipts note. |
| task-sources-cover-real-inputs | new | - | P1 | `code/03`, `agent/20`, tests-ci-04, frontend-rust-07, protocol-parity-04 | - | Merge agent/20 into code/03 (report 15.1). One issue: add missing `sources:` (CLI tests/templates/build.rs, embedded frontends, frontend sources for E2E, parity-matrix.yaml/PARITY.md/validate-parity.py, shatter-rust-runtime, rust-fe tests/, shatter-llm) and a meta test that fails when a tracked file feeding a test task is outside every task's sources. The affected-selector half of agent/20 goes to affected-gates-routing. Link str-qwua7.2. |
| affected-gates-routing | new | - | P2 | `code/04`, gates-06, tests-ci-05 | - | File code/04. Also carry the selector part of agent/20 / frontend-rust-07 (shatter-llm and rust-fe test paths must select their gates). Keep the e2e-runs-twice item out (it lives in collapse-test-tiers). |
| fast-hermetic-precommit | new | - | P1 | `code/83`, sessions-04 | D4 | File code/83 as is. Link str-35vtk.24/.25 and str-jttrf. Do not add any hook-bypass guidance (D4); the goal is a fast hook, not a documented way around it. |
| wire-every-test-module | new | - | P2 | `agent/19`, tests-ci-10, protocol-parity-05 | - | File agent/19 as is (meta test: every scripts/test_*.py and demo/test_*.py is run by some gate). Verifier correction: gate-receipt.py's unit tests do run in meta; drop that example. |
| gauntlet-scan-checker-consumes-json | new | - | P1 | `agent/09`, artifacts-07 | - | File agent/09 at P1 (report 15.1 and 14 item 25 override the P2 in the report tables). Checker consumes scan JSON `failed[]` instead of the dead regex; fix the tests that pin the dead format; update the allowlist and CLAUDE.md wording. |
| gauntlet-checker-reopen-note | reopen-note | str-jeen.57 | P1 | artifacts-07 | - | Comment on closed str-jeen.57 (and mention str-jeen.59 / str-qwua7.10): the scan-failure check has matched nothing since 2026-05-13; point to the new gauntlet-scan-checker-consumes-json issue. Do not reopen. |
| verifier-per-language-evidence | new | - | P2 | `agent/18`, tests-ci-06 | - | File agent/18. agent-repo-11 (duplicate-open str-qwua7.55) is context only: add one line on str-qwua7.55 linking this issue instead of a separate note. Verifier correction: the str-qwua7.4 example is overstated (the changed Go test was run directly); drop it. |
| gate-telemetry-executed-vs-cached | new | - | P2 | `code/10`, gates-11 | - | File code/10 as is (telemetry distinguishes executed vs cached runs; sccache unused; machine-wide slot covers only shatter gates). |

### shatter-ci-workflows (shatter, 14 entries)

GitHub workflows that are permanently red or ungated: release matrix, drift patrol, workflow health, linters/formatters, CI user paths.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| release-windows-z3-build | new | - | P1 | `code/70`, tests-ci-02, prior-02 | D1 | Split from code/70. D1: keep x86_64-pc-windows-msvc in the matrix and fix it (z3-sys cannot find z3.h: use z3-sys bundled/static-link-z3 or vcpkg). Remove every 'or drop from the matrix' option. Close only with the URL of a release.yml run where the Windows job is green. |
| release-aarch64-openssl-cross | new | - | P1 | `code/70`, tests-ci-02, prior-02 | D1 | Split from code/70. D1: keep aarch64-unknown-linux-gnu and fix openssl-sys under cross (rustls, vendored openssl, or restored Cross.toml pre-build; note str-qwua7.41 deleted Cross.toml/cross/ on a false premise). No drop option. Close only with a green release-run URL for the aarch64 job. |
| release-publish-and-install-smoke | new | - | P1 | `code/70`, tests-ci-02, prior-02 | D1 | Split from code/70: the release job publishes the continuous-* prerelease, and a CI step smoke-tests install.sh and action.yml against it. Blocked by release-windows-z3-build and release-aarch64-openssl-cross (D1: the full five-target matrix ships). Close only with a green release.yml run URL and a green install smoke run. |
| release-reopen-note | reopen-note | str-lj7s | P1 | tests-ci-02 | - | Comment on closed str-lj7s: it was closed on 'landed' with no green run; 0/266 release runs succeeded. Point to the three release issues. |
| drift-patrol-workflow-go-mod | new | - | P1 | `agent/02`, prior-01, agent-repo-02, docs-23 | - | File agent/02 as a new issue (not a reopen). Fold in docs-23 (P3): DRIFT-PATROL.md's check table omits the tracker-server check. Also fix the table while touching the workflow. If the workflow reads .beads/issues.jsonl, coordinate with beads-jsonl-consumers-drop-bd-sync (D4). |
| drift-patrol-reopen-note | reopen-note | str-u394l.1 | P1 | prior-01 | - | Comment on closed str-u394l.1: the scheduled workflow has failed 7/7 runs (setup-go points at a nonexistent root go.mod). Point to drift-patrol-workflow-go-mod. |
| workflow-health-patrol | new | - | P1 | `agent/03`, agent-repo-03, tests-ci-03 | D1 | File agent/03 at P1 (verifier kept agent-repo-03 at P1; report 15.1). Blocked by drift-patrol-workflow-go-mod. D1: remove 'or those workflows/targets are disabled with a documented reason' for the release targets; the release fixes are already filed in this bucket, so link them instead of filing more. perf-ci, devcontainer and docker-publish follow-up issues are still in scope. |
| go-lint-and-gofmt-gated | new | - | P2 | `code/57`, `agent/25`, prior-08, frontend-go-05 | - | Merge code/57 and agent/25 (report 15.1). One issue: fix the 10 golangci-lint findings and 9 non-gofmt files, then gate golangci-lint and gofmt in check-static. |
| go-lint-reopen-note | reopen-note | str-2tyfk | P2 | prior-08 | - | Comment on closed str-2tyfk (and on str-qwua7.32): closed with residuals and a false 'lint passes'; point to go-lint-and-gofmt-gated. |
| rustfmt-gate | new | - | P2 | `agent/26`, sessions-11 | - | File agent/26. Must be done as one dedicated formatting commit then a `cargo fmt --check` gate (memory records that the tree is not rustfmt-clean; do not mix with other changes). |
| rustfmt-reopen-note | reopen-note | str-fr1v | P2 | sessions-11 | - | Comment on closed str-fr1v: the tree drifted again (58-file churn on 09-2x); point to rustfmt-gate. |
| ci-runs-user-paths | new | - | P2 | `code/73`, tests-ci-11 | - | File code/73 as is (smoke/walkthrough/gauntlet/E2E user paths in CI; Perf CI is 0/13). |
| workflow-action-versions | new | - | P3 | `code/77`, tests-ci-18 | - | File code/77 as is. |
| nextest-ci-profile-and-stale-parity-fallback | new | - | P3 | `code/07`, gates-08, tests-ci-07 | - | File code/07 at P3. Verifier correction for tests-ci-07: in-process mutexes do serialize under plain cargo test; keep only the dead-config and divergence points. |

### shatter-test-hygiene (shatter, 9 entries)

Test-suite reliability and hygiene: snapshot helpers, pinned inputs, /tmp leaks, tier sprawl, stale failfiles, fuzz policy vs reality.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| snapshot-test-helpers | new | - | P2 | `code/71`, tests-ci-08 | - | File code/71 as is. |
| pin-examples-repo | new | - | P2 | `code/72`, tests-ci-09 | - | File code/72 as is. |
| tests-leak-tmp-dirs | new | - | P2 | `code/84`, sessions-08 | - | File code/84. Link open str-dl2pj (config discovery boundary) as related, not duplicate. Verifier correction: executor.rs:856 is bin-only. |
| ts-handlers-test-timeouts | new | - | P3 | `code/06`, gates-05 | - | File code/06 as is. |
| collapse-test-tiers | new | - | P3 | `code/74`, tests-ci-13, gates-07 | - | File code/74 at P3. It owns the 'e2e runs twice in pre-completion-e2e' item; remove that item from test-tier-docs (docs-ui/21) so it is filed once (15.1). |
| rapid-failfile-purge | new | - | P3 | `code/75`, tests-ci-15, prior-23 | - | File code/75 as is. |
| rapid-failfile-reopen-note | reopen-note | str-qwua7.4 | P3 | tests-ci-15, prior-23 | - | Comment on closed str-qwua7.4: a March rapid failfile is still tracked and .gitignore covers only planner/; point to rapid-failfile-purge. |
| broad-run-gate-duplicates | new | - | P3 | `code/76`, tests-ci-16 | - | File code/76 as is. |
| fuzz-policy-vs-reality | new | - | P3 | tests-ci-14 | - | NEW (no draft). formal-methods-policy skill prescribes cargo-fuzz and Go native fuzz jobs; reality is seed-only Go fuzz and no cargo-fuzz. Either add the fuzz targets and a scheduled job, or change the skill to match. Check whether str-df9g was closed unfixed; cite str-l02k/str-aslo. Evidence: areas/tests-ci.md T-14. |

### shatter-engine-correctness (shatter, 12 entries)

Core engine wrong answers: solver sort split, path counting, setup/mocks under concolic, shrinkers, refine phase, invariants, solver timeouts.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| z3-mixed-int-real-sort-split | new | - | P1 | `code/11`, core-01 | - | File code/11 as is. |
| float-probe-paths-uncounted | new | - | P1 | `code/12`, `docs-ui/02`, core-02, cli-ux-15, artifacts-03, goals-03, prior-19 | - | Keep code/12 (root cause); drop docs-ui/02 as a separate issue and paste its report-level evidence (goals-03/prior-19 examples) into code/12. Acceptance also asserts report path count == spec class count (14 item 5). |
| concolic-setup-teardown | new | - | P1 | `code/13`, core-03, core-09 | - | File code/13 as a new issue citing closed str-0s76.6 (report 15.1). |
| setup-parity-reopen-note | reopen-note | str-0s76.6 | P1 | core-03 | - | Comment on closed str-0s76.6: every production caller passes setup_context=None, so --setup is ignored under concolic; point to concolic-setup-teardown. |
| concolic-mock-variation-regression | new | - | P2 | `code/14`, core-04 | - | File code/14 at P2 (verifier corrected core-04 P1 -> P2). |
| mock-variation-reopen-note | reopen-note | str-3ky9.4 | P2 | core-04 | - | Comment on closed str-3ky9.4: undone by str-lebv/str-r59s; point to concolic-mock-variation-regression. |
| duplicate-value-shrinkers | new | - | P2 | `code/15`, core-05 | - | File code/15. Link open str-v0yjq (14 says retarget it: its enum_values shrink work must target live shrink.rs, not input_gen) and closed str-55ep/str-ddxe. |
| concolic-refine-execute-builder | new | - | P2 | `code/16`, core-06, core-16 | - | File code/16. core-16 is duplicate-open str-qwua7.5: link it, do not re-describe the capture flag. |
| invariant-min-support | new | - | P2 | `code/20`, core-11 | - | File code/20 as is. |
| z3-default-query-timeout | new | - | P2 | `code/21`, core-12 | - | File code/21 as is (includes the discarded `scan --solver-timeout`). |
| qwua7-49-rescope | note-to-existing | str-qwua7.49 | P2 | `code/19`, core-10 | - | Post code/19 as a comment on str-qwua7.49: the try_send/None premise is refuted; the real failure is an empty worker pool misreported as task time. |
| float-constant-rational-conversion | new | - | P3 | `code/24`, core-19 | - | File code/24 as is. |

### shatter-concolic-and-engine-design (shatter, 10 entries)

Measure concolic before positioning it (D3), engine parity, path/budget semantics, dead code, and effectiveness measurement.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| concolic-vs-default-benchmark | new | - | P1 | `code/80`, goals-08 | D3 | From code/80, raised to P1 (D3). Controlled default-vs-concolic benchmark: fixed seeds, fresh artifacts (no resume; see explore-resume-options-key), the examples corpus plus one downstream project (kapow, zolem or pickpackit), results published per release. Remove the 'README/SPEC positioning matches' criterion and the early-termination item (own issues). Verifier: the default baseline (40.7%) was not reproduced; the benchmark must re-measure it. |
| concolic-early-termination | new | - | P1 | goals-08 | D3 | NEW (split from code/80, D3). Concolic stops after ~21-35 iterations on most hard TS functions (goals-runs/ts-sub-concolic.err: computeArea 21 iters 0/6 branches, matchRoute 21 1/19, negotiateLanguage 23 1/12, classifyStatus 21 0/3). Find the root cause (worklist exhaustion, plateau detection, or unsat/unknown handling) and fix it; add a known-answer test that runs past the old stop point. Some losses are downstream of z3-mixed-int-real-sort-split and ts-switch-ternary-instrumentation; link them. |
| concolic-positioning-decision | new | - | P2 | goals-08 | D3 | NEW decision issue (D3). Blocked by concolic-vs-default-benchmark and concolic-early-termination. Once both land, re-decide whether README/SPEC keep 'concolic-first' positioning, using the benchmark numbers. Do not draft doc softening now. |
| effectiveness-benchmark-holdout | new | - | P2 | `other-first-party/50`, goals-10 | - | File other/50 in the shatter tracker (shatter-effectiveness has no tracker; holdout's bd is empty). Relate to concolic-vs-default-benchmark; they can share a harness but measure different things (bug-finding vs coverage). |
| engine-path-identity-budget-config | new | - | P2 | `code/17`, core-07, core-14 | - | File code/17. Link str-qwua7.6.2 and str-qwua7.20.1. |
| engine-parity-e2e | new | - | P2 | `agent/23`, core-22 | - | Split agent/23: this issue keeps the engine_parity E2E suite (fixtures x {random, concolic}), the `_`-prefixed-param lint, and the close-reason call-site rule. The per-BranchType TS fixtures move to ts-branchtype-known-answer-fixtures. |
| core-dead-code-removal | new | - | P2 | `code/18`, core-08 | - | File code/18. Include dropping array_mutation from str-qwua7.47's proptest list (core-17 is duplicate-open). |
| stable-hash-persisted-keys | new | - | P3 | `code/25`, core-20, frontend-rust-13 | - | File code/25 as is. |
| qwua7-6-function-length-ratchet | note-to-existing | str-qwua7.6 | P3 | `code/23`, core-15 | - | Post code/23 as a comment on str-qwua7.6 (explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet). |
| qwua7-43-bench-dev-dep-cycle | note-to-existing | str-qwua7.43 | P2 | `agent/31`, frontend-rust-09 | - | Post agent/31 as a comment on str-qwua7.43 (bench_frontier_ranking.rs deepened the core->shatter-llm dev-dependency cycle). |

### shatter-artifacts-correctness (shatter, 14 entries)

Artifacts that are wrong or missing: explore resume, -o JSON bundles, mixed-language scan, revalidate, snapshot diff retirement, coverage metrics, file names.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| explore-resume-options-key | new | - | P1 | `code/22`, artifacts-01, core-13, cli-ux-16, goals-05, prior-18, frontend-go-12 | - | File code/22 (key resume on an options hash, label resumed results, isolate walkthrough steps). Link open str-jd0d1 and closed str-8q1b4. |
| explore-o-json-empty-bundle | new | - | P1 | `code/26`, cli-ux-01, artifacts-02 | - | Split code/26 per 14 item 8: this issue covers `explore -o out.json` / --spec-out writing an empty no_targets bundle after a successful single-file run. |
| multi-file-spec-bundle-first-only | new | - | P1 | `code/26`, goals-04 | - | Split code/26: multi-file and glob targets keep only the first file's bundle; a glob writes a no_targets marker. Blocked by explore-o-json-empty-bundle if they share the writer. |
| mixed-language-scan-deletes-artifacts | new | - | P1 | `code/34`, artifacts-04, docs-07 | - | File code/34 (code half; checkpoint under scan_root). The SPEC 6 doc half is spec-s6-layout-and-checkpoint, blocked by this issue. |
| revalidate-return-values | new | - | P1 | `code/39`, goals-02 | - | File code/39 as a new issue citing str-kab3/str-3lob. |
| revalidate-reopen-note | reopen-note | str-kab3 | P1 | goals-02 | - | Comment on closed str-kab3 (and str-3lob): revalidate ignores return values and counts path changes as confirmed; point to revalidate-return-values. |
| retire-snapshot-diff | new | - | P1 | `code/38`, goals-01, docs-02, artifacts-12 | D2 | Rewrite code/38 per D2: no decision left open. Remove the `shatter diff` snapshot command and the unused Snapshot writer/reader path (snapshot.rs from_behavior_map/write_to_file, diff.rs), and make spec-diff the documented regression tool in SPEC (2.6, 5.5), README and QUICKSTART 5. Delete option (a) (snapshot producer). State in the body that the `diff` name becomes free and that str-81xiw decides whether diff-scoped exploration takes it; this issue does not decide that. |
| snapshot-diff-reopen-note | reopen-note | str-6k6.1 | P1 | goals-01, artifacts-12 | D2 | Comment on closed str-6k6.1: it shipped only the reader; the command is being retired (D2). Point to retire-snapshot-diff. |
| diff-name-freed-note | note-to-existing | str-81xiw | P2 | goals-01 | D2 | Comment on open epic str-81xiw: `shatter diff` (snapshot) is being retired in retire-snapshot-diff, so the `diff` name becomes free. Whether diff-scoped exploration takes it is for str-81xiw to decide. Informational only; do not change its scope. |
| go-scan-coverage-clamp | new | - | P1 | `code/78`, goals-06 | - | Split code/78 per 14 item 11: remove the silent clamp that inflates Go scan coverage to 100% with 7/18 branches, plus a cross-language coverage test. |
| rust-instrumentable-line-count | new | - | P1 | `code/78`, prior-20, goals-06 | - | Split code/78: the Rust frontend reports instrumentable_line_count so fully covered functions stop showing ~54% (Go fixed in str-szcn3). P1 because goals-06 (P1) covers it; prior-20 alone was P2. |
| behavior-map-cache-keys | new | - | P2 | `code/32`, cli-ux-12, goals-14 | - | File code/32 as is. |
| scan-artifact-filenames-abs-path | new | - | P2 | `code/29`, cli-ux-07 | - | File code/29 as is. |
| qwua7-39-json-stdout-first-run | note-to-existing | str-qwua7.39 | P1 | `docs-ui/03`, cli-ux-04, frontend-go-12, docs-15 | - | Post docs-ui/03 as a comment on str-qwua7.39 and raise it to P1 (cli-ux-04 verified P1; 14 item 22): first-run `scan --format json` stdout is not JSON; widen to every JSON command; empty init path. docs-15 (duplicate-open) is covered by this note. |

### shatter-reports-and-specs (shatter, 12 entries)

Report and spec rendering/contract quality: invariants, YAML tags, scan headline, branch metric, spec shapes and preconditions, golden outputs.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| invariant-markdown-blank-subjects | new | - | P2 | `docs-ui/15`, artifacts-05 | - | File docs-ui/15 at P2 (verifier corrected artifacts-05 P1 -> P2). |
| spec-yaml-custom-tags | new | - | P2 | `docs-ui/16`, artifacts-14 | - | File docs-ui/16 as is. |
| scan-report-headline-and-paths | new | - | P2 | `docs-ui/17`, artifacts-15, artifacts-13 | - | File docs-ui/17 (artifacts-13 folded in). Verifier: the markdown headline states its subset; only the HTML tile misleads. Coverage gap: the HTML was read as text only; acceptance should include rendering it (bugshot or screenshot). |
| branch-metric-counts-sites | new | - | P2 | `docs-ui/20`, core-18, frontend-go-12 | - | File docs-ui/20 as is. |
| spec-json-shapes-compare | new | - | P2 | `code/37`, artifacts-09, docs-03 | - | File code/37 (code half of docs-03). |
| spec-preconditions-from-path-constraints | new | - | P2 | `code/36`, artifacts-08, goals-13 | - | File code/36 as is. |
| qwua7-38-spec-diff-false-negative | note-to-existing | str-qwua7.38 | P2 | `code/35`, artifacts-06 | D2 | Post code/35 as a comment on str-qwua7.38 but do NOT raise it to P1: the verifier corrected artifacts-06 to P2. spec-diff is now the only regression tool (D2), so mention that it matters more. |
| golden-and-consumer-suite | new | - | P2 | `agent/24`, artifacts-18 | - | File agent/24; blocked by gauntlet-scan-checker-consumes-json. Verifier: insta snapshot tests exist but are synthetic and pin current bugs; say so. |
| known-answer-ratchet-and-ts-discriminants | new | - | P2 | `code/79`, goals-07 | - | File code/79 as is. |
| source-bucket-fixture-dir | new | - | P3 | `docs-ui/18`, goals-16 | - | File docs-ui/18 as a new issue citing str-9awj. |
| source-bucket-reopen-note | reopen-note | str-9awj | P3 | goals-16 | - | Comment on closed str-9awj: same class recurs for production packages named `fixture`; point to source-bucket-fixture-dir. |
| control-bytes-in-reports | new | - | P3 | `docs-ui/19`, goals-17 | - | File docs-ui/19 as is. |

### shatter-cli-flags-and-help (shatter, 10 entries)

CLI flag semantics and help text: --format, duplicate output, execution-only flags, --seed, config typos, tracker IDs in help, telemetry list, polish.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| explore-format-flag-ignored | new | - | P2 | `code/27`, cli-ux-02 | - | File code/27 at P2 (verifier corrected cli-ux-02 P1 -> P2). |
| explore-report-printed-twice | new | - | P2 | cli-ux-03 | - | NEW (no draft; 15.1). `-o FILE --stdout` prints the explore report twice; `-q -o FILE` leaks the header to stdout. Related str-zt4v/str-6c6p/str-xve. Add CLI output tests for each combination of -o, --stdout and -q. Evidence: areas/cli-ux.md F 03 and cli-ux-transcripts/. |
| help-hides-execution-flags | new | - | P2 | `code/28`, `docs-ui/07`, cli-ux-05, prior-07, docs-22 | - | Merge code/28 (design fix: replace the argv-scanning workaround with per-command flag definitions) and docs-ui/07 (symptom: `help <cmd>` and 9 non-executing commands still show --allow-host-writes/--set). docs-22 (P3) is covered. |
| help-flags-reopen-note | reopen-note | str-qwua7.15 | P2 | cli-ux-05, prior-07 | - | Comment on closed str-qwua7.15: `shatter help spec-diff` still shows execution-only flags; point to help-hides-execution-flags. |
| seed-for-explore-and-run | new | - | P2 | `code/30`, cli-ux-08 | - | File code/30 as a new issue. |
| seed-reopen-note | reopen-note | str-0m0vn | P2 | cli-ux-08 | - | Comment on closed str-0m0vn: its symptom named explore but --seed exists only on scan; point to seed-for-explore-and-run. |
| unknown-config-keys-warn | new | - | P2 | `code/31`, cli-ux-09 | - | File code/31 as is. |
| help-tracker-ids-lint | new | - | P2 | `agent/28`, `docs-ui/08`, cli-ux-17, docs-18 | - | Merge docs-ui/08 into agent/28 at P2 (15.1). One issue: remove tracker IDs and internal notes from --help, SPEC and README, add a lint, and file the unfiled 2026-09-04 UI items. Verifier: help IDs count is 11-13. |
| telemetry-known-subcommands | new | - | P3 | `code/33`, cli-ux-18 | - | File code/33 as is. |
| cli-minor-output-and-help-polish | new | - | P3 | `docs-ui/13`, `docs-ui/14`, artifacts-16, goals-18, cli-ux-19 | - | Merge the two small P3 drafts docs-ui/13 (minor output defects) and docs-ui/14 (--analyze-only refused without a sandbox; error/help polish) into one issue. |

### shatter-cli-runtime-output (shatter, 8 entries)

What users see while and after running: sandbox guard bypass, scan progress, runtime-path hints, per-language errors, run report verdict and metrics.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| sandbox-backend-disables-guard | new | - | P1 | `docs-ui/01`, docs-01 | - | File docs-ui/01 (code half NEW: SHATTER_SANDBOX_BACKEND must not turn off the TS/Rust host-write guard; docs half fixes README/QUICKSTART remedies). |
| docs-first-run-reopen-note | reopen-note | str-qwua7.8 | P1 | docs-01, docs-05 | - | One comment on closed str-qwua7.8 covering both findings: the recommended sandbox remedy disables write protection (docs-01), and the SPEC changelog backfill claims 2.8/2.9/3.6 updates that were never made (docs-05). Point to sandbox-backend-disables-guard and spec-changelog-backfill. |
| scan-progress-post-hoc | new | - | P2 | `docs-ui/09`, cli-ux-06 | - | File docs-ui/09 as a new issue. |
| scan-progress-reopen-note | reopen-note | str-7pkp.5 | P2 | cli-ux-06 | - | Comment on closed str-7pkp.5: scan progress prints after the scan ends; point to scan-progress-post-hoc. |
| rust-runtime-path-and-doctor | new | - | P2 | `docs-ui/10`, cli-ux-10 | - | File docs-ui/10 as is. |
| per-language-outcome-rendering | new | - | P2 | `docs-ui/11`, cli-ux-11 | - | File docs-ui/11 as is. |
| run-report-verdict-and-coverage-metrics | new | - | P2 | cli-ux-13 | - | NEW (no draft; 15.1). The run report opens with an unexplained 'degraded' verdict before its H1, and explore, scan and run use three different coverage metrics. Acceptance: verdict explained under the H1 with its cause; one named coverage metric shared by all three (or each labelled). Related str-jeen.5, str-qwua7.57, str-4ad5. Evidence: areas/cli-ux.md F 13. |
| markdown-drops-render-plain-info | new | - | P3 | `docs-ui/12`, cli-ux-14 | - | File docs-ui/12 as is. |

### shatter-frontend-ts (shatter, 14 entries)

TypeScript frontend correctness and tests: flow map, switch/ternary instrumentation, shadowing, timeouts, request validation, parity tests.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| ts-flow-map-program-point | new | - | P1 | `code/40`, frontend-ts-01 | - | File code/40 as is (includes the constraint-consistency oracle). |
| ts-switch-ternary-instrumentation | new | - | P1 | `code/41`, frontend-ts-02, prior-17 | - | File code/41 as a new issue citing str-wsg. |
| ts-branches-reopen-note | reopen-note | str-wsg | P1 | frontend-ts-02 | - | Comment on closed str-wsg: switch/ternary/value-position &&,// are analyzed but never instrumented; point to ts-switch-ternary-instrumentation. |
| ts-shadowed-callback-params | new | - | P2 | `code/42`, frontend-ts-04 | - | File code/42 as is. |
| ts-timeout-classification | new | - | P2 | `code/43`, frontend-ts-05 | - | File code/43 as is. |
| ts-request-validation | new | - | P2 | `code/44`, frontend-ts-07 | - | File code/44 as is. |
| rf2v-fourth-walker-and-analyze-dataflow | note-to-existing | str-rf2v | P2 | `code/45`, `docs-ui/23`, frontend-ts-03, frontend-ts-09 | - | Post ONE combined comment on str-rf2v from code/45 (fourth SSA walker in executor.ts) and docs-ui/23 (analyzer ignores data flow; parity matrix wrongly says TS analyze produces ite) (15.1). |
| ts-protocol-and-parity-tests-meaningful | new | - | P2 | `code/46`, frontend-ts-10, frontend-ts-11, tests-ci-12 | - | File code/46. Verifier correction for tests-ci-12: a buildSymExpr/WithFlow parity describe block exists; the gap is narrower (only handled node kinds, unknown vs non-unknown). |
| ts-branchtype-known-answer-fixtures | new | - | P2 | `agent/23`, frontend-ts-18 | - | Split from agent/23: one TS known-answer fixture per BranchType asserting analyze (id, line) == instrument (id, line) and both outcomes discovered. File the test workarounds recorded in prose as issues. |
| mhinv-3-planner-probe-not-supported | note-to-existing | str-mhinv.3 | P2 | frontend-ts-08 | - | NEW note (no draft; 15.1). Comment on open str-mhinv.3: shatter-ts/CLAUDE.md promises `not_supported` for planner-command probes but TS returns `invalid_request` 'Unknown command'. Add it as an acceptance item (either behaviour changes or the contract does). |
| qwua7-31-eslint-evidence | note-to-existing | str-qwua7.31 | P2 | frontend-ts-12 | - | Comment on open str-qwua7.31 with the material new evidence: typescript-eslint finds 72 production issues including dead code and a default export. Verifier: the ts-conventions skill says 'prefer ESLint', not that it is enforced (the .31 title overstates). |
| ts-preflight-node-modules | new | - | P3 | `code/47`, frontend-ts-13 | - | File code/47 at P3 (verifier). |
| ts-operators-collapse-to-unknown | new | - | P3 | `code/48`, frontend-ts-14 | - | File code/48 as is. |
| ts-lifecycle-and-packaging-hygiene | new | - | P3 | `code/49`, frontend-ts-06, frontend-ts-16, frontend-ts-17 | - | File code/49. Verifier: `node dist/bundle.js` works in a normal dist; only a standalone bundle fails. |

### shatter-frontend-go (shatter, 13 entries)

Go frontend and go-tool wrapper: relocatable runtime, module path, rune literals, config discovery, build timeout, dead code, divergences.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| go-harness-runtime-embed | new | - | P1 | `code/50`, frontend-go-01 | - | File code/50 as is (embed the runtime; relocation and release smoke tests). |
| go-tool-module-path | new | - | P1 | `code/51`, frontend-go-02 | - | File code/51 as a new issue citing str-fl9g.2. |
| go-tool-reopen-note | reopen-note | str-fl9g.2 | P1 | frontend-go-02 | - | Comment on closed str-fl9g.2: documented `go get -tool .../go-tool/cmd/shatter` cannot resolve (module path vs shatter-go-tool/ directory); point to go-tool-module-path. |
| go-rune-and-escape-literals | new | - | P1 | `code/52`, frontend-go-03 | - | File code/52 as is (link str-qwua7.35). |
| qwua7-35-four-go-builders | note-to-existing | str-qwua7.35 | P2 | `code/53`, frontend-go-04 | - | Post code/53 as a comment on str-qwua7.35. |
| go-config-discovery-unbounded | new | - | P2 | `code/05`, gates-03, frontend-go-07 | - | File code/05. It is the root cause of open str-k7czv (close k7czv as a duplicate when this lands) and is the Go counterpart of open str-dl2pj; link both. |
| go-dead-code-and-property-targets | new | - | P2 | `code/54`, frontend-go-06, frontend-go-11 | - | File code/54 (re-scope open str-qwua7.48 to live generators; comment on .48 linking this issue). |
| go-tool-wrapper-robustness | new | - | P2 | `code/55`, frontend-go-10 | - | File code/55. Verifier: the GitHub API request does read GITHUB_TOKEN; only the download ignores it. |
| go-build-timeout-ignored | new | - | P2 | frontend-go-08 | - | NEW (no draft; 15.1). --build-timeout / SHATTER_BUILD_TIMEOUT and SHATTER_HARNESS_RELEASE are ignored by the Go frontend; `go build` has no timeout (apparently lost since str-9smo). Acceptance: the Go build honours the timeout with a test that a hanging build is killed; SHATTER_HARNESS_RELEASE honoured or documented as Rust-only. Related str-qwua7.20.2. Evidence: areas/frontend-go.md go-08. |
| go-connection-failures-divergence | new | - | P2 | `docs-ui/27`, protocol-parity-11 | - | File docs-ui/27 as is. |
| go-explore-warmup-gate | new | - | P3 | `code/81`, goals-12 | - | File code/81 at P3. |
| go-small-correctness-tidy | new | - | P3 | `code/56`, frontend-go-14, frontend-go-15, prior-22 | - | File code/56 as is. |
| go-cgo-refusal-covers-bodies | new | - | P3 | frontend-go-13 | - | NEW (no draft; 15.1). The cgo 'detect and refuse' claim covers signatures only, not C.* calls in function bodies. Either detect body calls or narrow the claim in shatter-go/CLAUDE.md and the parity matrix. Related str-hy9b.H5. Evidence: areas/frontend-go.md go-11. |

### shatter-frontend-rust (shatter, 12 entries)

Rust frontend, runtime and shatter-llm: crate-bridge stdout, constraint encoding, timeout budgets, panic boundary, usize inputs, duplicated tests.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| rust-crate-bridge-stdout | new | - | P1 | `code/58`, frontend-rust-01 | - | File code/58 as is (separate protocol channel). |
| rust-instrument-constraints | new | - | P2 | `code/59`, frontend-rust-02, frontend-rust-03 | - | File code/59; link open str-qwua7.36 (typed SymExpr) as the approach for the JSON half. |
| timeout-budget-invariant | new | - | P2 | `code/60`, frontend-rust-04 | - | File code/60 as is. |
| qwua7-50-panic-boundary | note-to-existing | str-qwua7.50 | P2 | `code/61`, frontend-rust-05 | - | Post code/61 as a comment on str-qwua7.50. Verifier: a unit test can exercise poison recovery, but production cannot survive a panic without a handler boundary. |
| rust-usize-negative-inputs | new | - | P2 | `code/82`, goals-15 | - | File code/82 as a new issue. |
| rust-usize-reopen-note | reopen-note | str-ddxe | P2 | goals-15 | - | Comment on closed str-ddxe: usize parameters still get negative integers; point to rust-usize-negative-inputs. |
| rust-main-duplicate-module-tree | new | - | P2 | `code/62`, frontend-rust-06 | - | File code/62 as is (587 unit tests run twice). |
| rust-tests-offline-silent-pass | new | - | P2 | `code/63`, frontend-rust-11 | - | File code/63 as is. |
| rust-frontend-design-dedupe | new | - | P3 | `code/64`, frontend-rust-12, frontend-rust-14 | - | File code/64 as is. |
| rust-runtime-harness-loop | new | - | P3 | `code/65`, frontend-rust-15 | - | File code/65 as is. |
| shatter-llm-hardening | new | - | P3 | `code/66`, frontend-rust-16, frontend-rust-17 | - | File code/66 as is. |
| qwua7-21-llm-seed-oracle-docs | note-to-existing | str-qwua7.21 | P3 | frontend-rust-18 | - | NEW note (no draft; 15.1). Comment on the open user-docs epic str-qwua7.21: add a user guide for the LLM seed oracle and an update policy for hard-coded default model IDs (or file it as a child of .21). |

### shatter-protocol-parity (shatter, 15 entries)

Protocol contracts and parity machinery: dispatch checks, conformance harness, single-source capabilities, schemas, governance and protocol docs, validator liveness.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| parity-dispatch-reconciliation | new | - | P2 | `code/67`, protocol-parity-01 | - | File code/67 at P2 (verifier corrected P1 -> P2). |
| conformance-harness-correctness | new | - | P2 | `code/09`, prior-05, protocol-parity-02, gates-10 | - | File code/09 at P2. |
| capability-single-source | new | - | P2 | `code/68`, protocol-parity-08, protocol-parity-19 | - | File code/68. If sizing exceeds ~2 days, split codegen for the 13 registry enums (protocol-parity-19, P3) into its own issue. |
| protocol-schemas-reject-real-output | new | - | P2 | `docs-ui/24`, protocol-parity-03 | - | File docs-ui/24 as is. |
| protocol-parity-md-stale | new | - | P2 | `docs-ui/25`, protocol-parity-06 | - | File docs-ui/25 as is. |
| governance-md-omits-matrix | new | - | P2 | protocol-parity-07 | - | NEW (no draft; 15.1). protocol/GOVERNANCE.md, the mandatory checklist for protocol changes, omits parity-matrix.yaml, codegen and validate-parity, and names two authorities. Rewrite the checklist so one authority is named and every step a real change needs is listed. Related str-2fjn, str-qwua7.7, str-fpgb.9. |
| protocol-md-execute-fields | new | - | P2 | protocol-parity-09 | - | NEW (no draft; 15.1). PROTOCOL.md's execute section documents about half the request and response fields. Document every field (generate from the schema if capability-single-source lands first). Related str-iqta, str-2fjn. |
| divergence-tracking-issue-liveness | new | - | P2 | `agent/29`, protocol-parity-12 | - | File agent/29 as is. |
| validator-ts-extraction-empty | new | - | P2 | protocol-parity-15 | - | NEW. The dedupe said duplicate-open str-qwua7.7, but the verifier found .7 CLOSED (0655458b) with its 'empty extraction must fail' criterion unmet: TS extraction is still silently empty, and its direction conflicts with GOVERNANCE. Acceptance: validate-protocol-registry fails on empty extraction for any frontend, with a canary test. |
| validator-reopen-note | reopen-note | str-qwua7.7 | P2 | protocol-parity-15 | - | Comment on closed str-qwua7.7: the empty-extraction criterion is unmet for TS; point to validator-ts-extraction-empty. |
| validator-optional-command-warning | new | - | P3 | `code/69`, prior-24 | - | File code/69 as is. |
| protocol-rs-doc-comments | new | - | P3 | `docs-ui/26`, protocol-parity-10 | - | File docs-ui/26. Verifier: an exhaustive-match test for error codes already exists; only the comments are wrong. |
| protocol-test-doubles-relocate | new | - | P3 | `docs-ui/28`, protocol-parity-20 | - | File docs-ui/28 as is. |
| parity-guidance-skill-and-template | new | - | P3 | `agent/15`, protocol-parity-17, protocol-parity-18 | - | File agent/15 as is. |
| qwua7-37-premise | note-to-existing | str-qwua7.37 | P3 | `agent/30`, protocol-parity-14 | - | Post agent/30 as a comment on str-qwua7.37 (Go package free functions are solvable; the premise is wrong). |

### shatter-docs (shatter, 10 entries)

User and contributor documentation accuracy: SPEC changelog, artifact schemas and SPEC 5/6, crate CLAUDE.md facts, docs-smoke reach, tier table, stories and drift gates.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| spec-changelog-backfill | new | - | P2 | `docs-ui/04`, docs-05 | - | File docs-ui/04 as a new issue; the str-qwua7.8 reopen-note is shared with sandbox-backend-disables-guard (bucket shatter-cli-runtime-output). |
| artifact-json-schemas | new | - | P2 | `docs-ui/05`, artifacts-10 | D2 | Split docs-ui/05: generate JSON Schemas from the serde types (spec, scan report, explore artifact, summary/manifest/run-status) into protocol/schemas/artifacts/ with a gate. Drop the 'shatter diff has no producer' out-of-scope line; retire-snapshot-diff handles diff (D2). |
| spec-s5-contract-table-and-samples | new | - | P2 | `docs-ui/05`, docs-03, docs-08 | D2 | Split docs-ui/05: SPEC 5 producer/consumer table and rewritten samples (including the obsolete 5.1 default report). Blocked by artifact-json-schemas and spec-json-shapes-compare. Must describe spec-diff as the regression tool and not describe snapshot diff (D2). |
| spec-s6-layout-and-checkpoint | new | - | P2 | `docs-ui/06`, docs-07 | - | File docs-ui/06; blocked by mixed-language-scan-deletes-artifacts. |
| crate-claude-md-stale-facts | new | - | P2 | `docs-ui/22`, core-21, frontend-ts-15, frontend-go-09, frontend-rust-08, docs-24, protocol-parity-13 | - | File docs-ui/22. Also add shatter-go/CLAUDE.md:138 ('TS and Rust currently declare outcome only') from protocol-parity-13 and the stale .js references from docs-24, instead of separate notes on str-qwua7.24/.25. |
| docs-smoke-coverage | new | - | P3 | `docs-ui/29`, docs-19 | - | File docs-ui/29 as is. |
| test-tier-docs-overstate-coverage | new | - | P3 | `docs-ui/21`, gates-07, tests-ci-17 | - | File docs-ui/21 without the 'e2e runs twice' item (owned by collapse-test-tiers, 15.1). |
| qwua7-52-story-seed-list | note-to-existing | str-qwua7.52 | P2 | docs-11, goals-19 | - | Comment on open str-qwua7.52 with the audit's proposed seed story list (baseline -> change -> detect journeys; note it is the auditor's proposal, not the maintainer's) and that storystore's inventory finds 0 clap/cobra surfaces (storystore clap/cobra extractor issue). |
| wurp-changelog-row-and-reverse-check | note-to-existing | str-wurp | P2 | docs-06 | - | Comment on open str-wurp: add the changelog-row rule and the reverse (SPEC -> clap) flag check to its acceptance; flag-level SPEC drift keeps recurring. |
| u394l-3-pending-turns-fail | note-to-existing | str-u394l.3 | P2 | prior-21 | - | Comment on open str-u394l.3: drift-patrol PENDING checks unimplemented for 3+ months; proposal that a PENDING slot older than 60 days turns FAIL. |

### shatter-tracker-and-beads (shatter, 11 entries)

Tracker truth: retire the JSONL import and move to a Dolt remote (D4), reconcile stale issues, triage policy, publish audit reports, downstream goal ownership.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| beads-jsonl-import-clobber-check | new | - | P1 | `agent/05`, `agent/08`, prior-03, sessions-05, agent-repo-07, docs-16 | D4 | NEW (D4, first step). Root cause measured 2026-09-23: bd's post-checkout hook spends ~6 min 'importing JSONL from .beads/issues.jsonl' (1,773 issues, ~10 s CPU; waiting, not computing), and bd itself says the JSONL is an export, not sync or source of truth. The committed JSONL was frozen at 134dd616 (2026-09-07). Verify whether importing it on every checkout has overwritten newer DB state: diff live DB vs the JSONL vs Dolt history for status/priority/body of issues updated after 09-07 (agent/05 lists 19 known status mismatches such as str-qwua7.4/.7/.8/.9/.15); repair any clobbered issues and record the list. Use agent/05 and agent/08 only as evidence; drop agent/08's options (a) env var and (b) hook env block, and write no hook-bypass guidance. |
| beads-retire-jsonl-import-dolt-remote | new | - | P1 | `agent/08`, sessions-05, agent-repo-07, prior-03 | D4 | NEW (D4). Blocked by beads-jsonl-import-clobber-check. Stop the JSONL import in shatter (bd config / hook integration, not BEADS_HOOK_TIMEOUT), configure a Dolt remote (`bd dolt remote add origin ...`, `bd dolt push`/`pull`) as the cross-machine sync, and measure: `git worktree add` in shatter completes in < 15 s and land.py create_preview < 30 s (commands and times in the close reason). Record the new sync procedure once in AGENTS.md. |
| beads-jsonl-consumers-drop-bd-sync | new | - | P1 | `agent/05`, prior-03, agent-repo-06, docs-16 | D4 | NEW (D4), blocked by beads-retire-jsonl-import-dolt-remote. Rewrite agent/05 without its keep-or-untrack decision: AGENTS.md (lines ~125, 293, 340-369), .beads/PRIME.md and repo skills (audit SKILL.md:385) drop `bd sync` and point at the Dolt-remote procedure; CI drift-patrol tracker-hygiene and scripts/cleanup-merged-remote-branches.sh stop trusting the stale JSONL (read the Dolt remote, or SKIP with a reason); decide whether the JSONL stays tracked as a plain export; delete or explain .beads/issues.recovered.jsonl; add a docs check that every bd subcommand named in AGENTS.md/skills exists; state the expected bd version. agent-repo-06 verifier priority is P2, but D4 makes this part of the P1 retirement. |
| qwua7-28-superseded | note-to-existing | str-qwua7.28 | P2 | `agent/08`, sessions-05, agent-repo-07 | D4 | Comment on str-qwua7.28 and close it as superseded (D4): the BEADS_HOOK_TIMEOUT approach is not being pursued; the root cause is the JSONL import. Point to beads-retire-jsonl-import-dolt-remote. Also note its body's setup-hooks.sh:41 fact is stale since b5cd25ec. |
| ly5bz-superseded | note-to-existing | str-ly5bz | P3 | prior-03, agent-repo-06 | D4 | Comment on str-ly5bz: the bd sync cadence question is moot (D4 retires the JSONL import and bd sync); point to beads-jsonl-consumers-drop-bd-sync and close as superseded. |
| mpgg1-close | note-to-existing | str-mpgg1 | P2 | agent-repo-07, prior-10 | D4 | Comment on str-mpgg1 (in_progress, revert already merged 84941b37) and close it: the hook-env edits stay reverted, and D4 replaces the timeout debate. Include in the tracker-reconciliation sweep if filed together. |
| publish-audit-reports | new | - | P1 | `agent/06`, agent-repo-04, docs-04 | D4,D6 | File agent/06 at P1 (verifier kept agent-repo-04 at P1; 15.1). Land the 2026-09-04 report and this 2026-09-22 report on main before issues are filed that cite them. Its /audit skill step must not use `bd sync` (D4). Link str-qwua7.22 and str-qwua7.44 in the body; no separate note. |
| tracker-reconciliation-sweep | new | - | P2 | `agent/10`, agent-repo-05, sessions-09, prior-09, protocol-parity-16, frontend-rust-10 | - | File agent/10. Drop the memory part of frontend-rust-10 (memory already corrected 2026-09-23). Include closing str-qwua7.1-obsolete parts per prior-09 only after the str-qwua7.1 note below widens .1's check; link prior-12 (str-qwua7.62 execution). |
| triage-policy-and-audit-epic-waves | new | - | P2 | `agent/11`, prior-14, prior-25, agent-repo-18 | - | File agent/11 as is. |
| downstream-coverage-goals-epic | new | - | P2 | `agent/27`, goals-09 | - | File agent/27 at P2 (verifier corrected goals-09 P1 -> P2). |
| qwua7-17-drift-patrol-hygiene | note-to-existing | str-qwua7.17 | P3 | `code/08`, gates-09 | - | Post code/08 as a comment on str-qwua7.17. |

### shatter-agent-guidance-and-repo-hygiene (shatter, 11 entries)

Repo-level agent guidance and git hygiene: identity (D5), git-state checks, fixture incident, skills, env-doctor decisions, AGENTS.md rtk/landing prose, completion and planning rules.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| mailmap-and-fixture-config-snapshot | new | - | P1 | `agent/04`, agent-repo-01, prior-04 | D5 | Rewrite agent/04 per D5. The leaked [user] section was already removed on 2026-09-23: delete that acceptance item and any history-decision item. Scope: (1) add .mailmap mapping test@example.com with names 'Test' and 'Test User' to Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite, verified with `git log --use-mailmap`; (2) scripts/test_git_fixture_isolation.py snapshots $(git rev-parse --git-common-dir)/config before and after each fixture entrypoint and fails on any change. The git-state check goes to str-qwua7.1 (note below). |
| qwua7-1-git-state-check | note-to-existing | str-qwua7.1 | P1 | `agent/04`, agent-repo-01, prior-04, prior-09 | D5 | Comment on open str-qwua7.1 (D5): core.bare is already repaired, so re-scope .1 to the check: drift-patrol or setup-hooks.sh --check FAILs on a repo-local user.name/user.email override, any *@example.com identity, core.bare=true, or a repo-local core.hooksPath override; unit-tested. |
| qwua7-51-identity-root-cause | note-to-existing | str-qwua7.51 | P2 | `agent/04`, agent-repo-01 | D5 | Comment on str-qwua7.51: 'Owner: Test' came from the leaked repo-local [user] fixture identity (removed 2026-09-23), not a missing SessionStart identity. Re-scope or close accordingly. |
| fixture-corruption-incident-reverify | new | - | P2 | `agent/12`, agent-repo-16 | - | File agent/12. Deleting recovery and contaminated remote branches stays operator-confirmed. |
| agent-config-gitignore | new | - | P2 | `agent/13`, agent-repo-10 | - | File agent/13 in shatter (repo-level fix). Verifier: .agents/ does not exist; drop that bullet. 14 also lists a dotfiles-side 'narrow the global gitignore' fix; mention it in the body as a possible alternative rather than filing a dotfiles issue. |
| repo-skills-rot | new | - | P2 | `agent/14`, `agent/07`, agent-repo-14 | D4 | File agent/14. Add from agent/07 only the audit-skill step (grep project memory for bypass advice and claims that contradict AGENTS.md or repo state). Replace its 'defers to bd sync' fix with the D4 Dolt-remote procedure. |
| env-doctor-decisions | new | - | P2 | `agent/16`, agent-repo-15, plugins-08 | - | File agent/16. Orphan-directory removal stays operator-confirmed. The bgs-3tq priority raise is filed in the bugshot bucket. |
| qwua7-23-agents-md-rtk-and-landing | note-to-existing | str-qwua7.23 | P2 | `agent/17`, `docs-ui/30`, plugins-12, agent-repo-09, agent-repo-12 | D4 | Post ONE combined comment on str-qwua7.23 from agent/17 and docs-ui/30 (15.1): etiquette rules inside the rtk-managed block, the rtk 'always safe' text, landing prose that contradicts land.py, merged remote branches. Replace every `bd sync` mention in the suggested landing text with the D4 procedure. |
| completion-checklist-spec-docs | new | - | P2 | `agent/21`, docs-10 | - | File agent/21 as is. |
| planning-rules-location-and-open-decisions | new | - | P2 | `agent/22`, docs-12 | - | File agent/22 as is. |
| 35vtk-9-swarm-config | note-to-existing | str-35vtk.9 | P3 | agent-repo-19 | - | NEW note (no draft; 15.1). Comment on open str-35vtk.9: .claude/swarm-config.md is no longer read by bento swarm and the batch-landing claim is unbacked; update .9's scope. |

### bento-landing (bento, 14 entries)

land.py and land-work: preview ownership, verifier logs, post-push workflow results, merge-state ownership, progress, branch cleanup, skill restructure.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| e583-preview-owner-lock | note-to-existing | bento-e583 | P1 | `bento/90`, bento-01 | - | Post bento/90 on bento-e583 (confirmed mechanism, owner lock, --force-foreign, two-process test). |
| land-py-verifier-log-kept | new | - | P1 | `bento/02`, bento-02 | - | File bento/02 as a new issue. |
| verifier-log-reopen-note | reopen-note | bento-rdtn.4 | P1 | bento-02 | - | Comment on closed bento-rdtn.4: land.py still deletes the verifier log it reports as output_path; point to land-py-verifier-log-kept. |
| land-work-post-push-workflow-health | new | - | P1 | `bento/04`, tests-ci-03 | - | File bento/04 at P1 (verifier kept tests-ci-03 at P1; 15.1). Cross-repo counterpart of shatter workflow-health-patrol. |
| land-py-merge-abort-ownership | new | - | P2 | `bento/06`, bento-05 | - | File bento/06 as is. |
| land-py-invocation-progress-log | new | - | P2 | `bento/07`, bento-06 | - | File bento/07 as is. |
| landing-deletes-remote-branches | new | - | P2 | `bento/11`, bento-15, prior-06 | - | File bento/11 as is. |
| verifier-contract-migration | new | - | P2 | `bento/14`, bento-13 | - | File bento/14 as is. |
| land-work-skill-restructure | new | - | P2 | `bento/16`, bento-14 | - | File bento/16; link open bento-by8 (extends it). |
| eth-swarm-lead-lands-from-teammate | note-to-existing | bento-eth | P2 | `bento/15`, bento-12 | - | Post bento/15 on bento-eth. |
| dyp7-admission-control | note-to-existing | bento-dyp7 | P2 | `bento/18`, sessions-07 | - | Post bento/18 on bento-dyp7. |
| rebase-before-land-configurable | new | - | P3 | `bento/19`, bento-07 | - | File bento/19 at P3. Verifier: rebase-before-land is documented policy; frame it as a repo option, not a leftover. |
| merge-push-observability | new | - | P3 | `bento/20`, bento-18 | - | File bento/20 as is. |
| merge-message-and-stale-branch-nudge | new | - | P3 | `bento/21`, agent-repo-17 | - | File bento/21 as is. |

### bento-guards-doctor-tracker (bento, 14 entries)

Guards, doctor and tracker flow: git-guard bypasses, git-hook latency visibility, beads Dolt-remote guidance (D4), check-unpushed, doctor state, previews, closure, close evidence.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| git-guard-bypasses-and-false-positives | new | - | P1 | `bento/01`, sessions-02, bento-04, sessions-15 | - | File bento/01 as is. |
| git-guard-reopen-note | reopen-note | bento-rdtn.15 | P1 | sessions-02, bento-04 | - | Comment on closed bento-rdtn.15: the guard is bypassed by /usr/bin/git, wrapper prefixes, -C, cd and GIT_CONFIG env; point to git-guard-bypasses-and-false-positives. |
| git-hook-latency-visibility | new | - | P1 | `bento/03`, bento-03 | D4 | Rewrite bento/03 per D4: DROP the BEADS_HOOK_TIMEOUT hydration suppression for previews and the 'env timeout' remedy; write no hook-bypass guidance. Keep: time each git subprocess in launch-work-bootstrap and create-preview and warn when one exceeds 30 s naming the hook; fix the guard's slow-hook pointer so it cites a real section (the beads-issue-flow Dolt-remote guidance from beads-dolt-remote-guidance); doctor reports beads hook markers older than `bd version`. Related shatter beads-retire-jsonl-import-dolt-remote. |
| beads-dolt-remote-guidance | new | - | P1 | `bento/17`, bento-16, prior-03 | D4 | Rewrite bento/17 per D4, raised to P1 because git-hook-latency-visibility points at it: beads-issue-flow states that .beads/issues.jsonl is an export, not sync or source of truth; repos should not run a JSONL import on checkout; cross-machine sync uses a Dolt remote (`bd dolt remote add origin ...`, `bd dolt push`/`pull`); no `bd sync` (removed in bd 1.x). Doctor warns when a repo imports JSONL on checkout, has no Dolt remote configured, or its docs mention `bd sync`. Drop the 'land.py exports and commits the jsonl' option. |
| check-unpushed-overcount-and-blocks | new | - | P2 | `bento/05`, bento-11, sessions-10 | - | File bento/05 as is. |
| doctor-state-per-worktree | new | - | P2 | `bento/08`, bento-08 | - | File bento/08 as a new issue. |
| doctor-state-reopen-note | reopen-note | bento-rdtn.2 | P2 | bento-08 | - | Comment on closed bento-rdtn.2: doctor seen/decided state is per checkout, so linked worktrees get full nudges; point to doctor-state-per-worktree. |
| stale-previews-leak-and-scoping | new | - | P2 | `bento/09`, bento-09, sessions-17 | - | File bento/09 as is. |
| closure-orphan-worktree-dirs | new | - | P2 | `bento/10`, bento-10 | - | File bento/10 as is. |
| close-reason-evidence | new | - | P2 | `bento/12`, prior-13 | - | File bento/12 as is. |
| claim-branch-reconciliation | new | - | P2 | `bento/13`, prior-10, bento-17 | - | File bento/13 as is. |
| followups-as-siblings | new | - | P3 | `bento/22`, prior-11 | - | File bento/22 as is. |
| per-subagent-scratch-dirs | new | - | P3 | `bento/23`, cli-ux-20 | - | File bento/23 at P3. The verifier says the fix may belong in the shatter audit workflow prompt; keep it scoped to bento's swarm/audit templates. |
| a0nz-behavioural-probe | note-to-existing | bento-a0nz | P3 | `bento/91`, core-23 | - | Post bento/91 on bento-a0nz. |

### shatter-agents-plugin (shatter-agents, 11 entries)

Plugin skills that document CLI behaviour the engine does not have, plus contract tests, delegation, CI wiring and payload hygiene.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| withdraw-shatter-diff-skill | new | - | P1 | `other-first-party/21`, plugins-01, goals-11, artifacts-11 | D2 | Rewrite other/21 per D2: correct the skill to what exists today. No `shatter diff --staged` or diff-scoped command exists, and the snapshot `shatter diff` is being retired (shatter retire-snapshot-diff). Withdraw the shatter-diff skill and its pre-commit hook recipe from the published payload (or mark it unreleased with a requires marker) and point readers to `shatter spec-diff` for regression checks. Do not assume the future command name: str-81xiw decides whether it is `diff` or `diff-explore`. |
| sa-tyb-reopen-note | reopen-note | sa-tyb | P1 | plugins-01 | D2 | Comment on closed sa-tyb: only skill text landed and the documented command does not exist; point to withdraw-shatter-diff-skill. |
| recipes-marked-design-only | new | - | P1 | `other-first-party/22`, plugins-02 | - | File other/22 as is. |
| sa-yyt-reopen-note | reopen-note | sa-yyt | P1 | plugins-02 | - | Comment on closed sa-yyt: recipe discovery and stubs registry are documented but unimplemented; point to recipes-marked-design-only. |
| cli-contract-test | new | - | P2 | `other-first-party/23`, plugins-03 | - | File other/23 at P2 (verifier corrected plugins-03 P1 -> P2). |
| delegate-discovery-to-engine | new | - | P2 | `other-first-party/24`, plugins-10 | - | File other/24 as is. |
| sa-oio-wrapper-convention | note-to-existing | sa-oio | P2 | `other-first-party/25`, docs-09 | - | Post other/25 on sa-oio. |
| wire-shatter-ci-standalone | new | - | P2 | `other-first-party/26`, plugins-11 | - | File other/26 as is. |
| advise-taxonomy-payload | new | - | P2 | `other-first-party/27`, plugins-14 | - | File other/27 as is. |
| claude-md-imports-agents-md | new | - | P2 | `other-first-party/28`, plugins-17 | - | File other/28 as is. |
| close-agents-mirror-issues | new | - | P2 | `other-first-party/29`, plugins-16 | - | File other/29 (the shatter-agents half of plugins-16). |

### storystore-adoption-blockers (storystore, 4 entries)

What blocks storystore adoption in shatter: tracker migration, clap/cobra extractors, version bumps. Under 8 issues because buckets are single-repo.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| tracker-migration-and-agents-md | new | - | P2 | `other-first-party/31`, plugins-09 | - | File other/31. The v32->v53 migration needs maintainer approval and must be run before anything else in this bucket can be filed (bd writes are blocked). The filer script should stop with a clear message if the migration has not been done. |
| clap-cobra-extractors | new | - | P2 | `other-first-party/32`, plugins-05 | - | File other/32 at P2 (verifier corrected plugins-05 P1 -> P2). |
| ss-yoa-reopen-note | reopen-note | ss-yoa | P2 | plugins-05 | - | Comment on closed ss-yoa: the inventory still finds 0 CLI surfaces in shatter (commander.js only); point to clap-cobra-extractors. |
| automatic-version-bump | new | - | P2 | `other-first-party/33`, plugins-07 | - | File other/33 as is. |

### bugshot-tracker-and-payload (bugshot, 5 entries)

Bugshot tracker and payload hygiene. Under 8 issues because buckets are single-repo.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| close-bugshot-mirror-duplicates | new | - | P2 | `other-first-party/41`, plugins-16 | - | File other/41 as is (55 duplicate-prefix mirror issues). |
| bgs-3tq-raise-to-p2 | note-to-existing | bgs-3tq | P2 | plugins-08 | - | Raise bgs-3tq to P2 and comment: shatter str-qwua7.53 (P2) depends on it. |
| installed-cache-bloat-investigation | new | - | P3 | `other-first-party/42`, plugins-18 | - | File other/42 as an investigation. Verifier: node_modules cannot come from the git source (gitignored) and the recorded install SHA predates the fix; trace the cache before concluding bgs-3cz is unfixed. |
| bgs-3cz-pointer | note-to-existing | bgs-3cz | P3 | plugins-18 | - | Comment on closed bgs-3cz pointing to installed-cache-bloat-investigation. Do not reopen until the investigation confirms. |
| agents-md-structure-and-readme | new | - | P3 | `other-first-party/43`, plugins-20 | - | File other/43 as is. |

### dotfiles-global-guidance (dotfiles, 11 entries)

Global agent guidance and hooks: loading the required rules, waiting behaviour, bypass framing, plugin auto-update, rtk, memory lifecycle, validators, escalation.

| Slug | Kind | Existing | P | Sources | Decisions | Instructions |
|---|---|---|---|---|---|---|
| global-guidance-actually-loads | new | - | P1 | `other-first-party/01`, plugins-06, plugins-19 | - | File other/01 as is. |
| background-wait-rule-and-hook | new | - | P2 | `other-first-party/02`, sessions-01 | - | File other/02 at P2 (verifier corrected sessions-01 P1 -> P2). |
| never-recommend-bypass | new | - | P2 | `other-first-party/03`, sessions-06 | D4 | File other/03 as is. Consistent with D4 (no hook-bypass guidance anywhere). |
| first-party-plugin-autoupdate | new | - | P2 | `other-first-party/04`, plugins-04 | - | File other/04 as is. |
| rtk-head-range-compound | new | - | P2 | `other-first-party/05`, plugins-13 | - | File other/05 as a follow-up to closed dotfiles#11 (reference it in the body). |
| memory-lifecycle-rule | new | - | P2 | `other-first-party/06`, plugins-15 | - | File other/06. The shatter memory files it cites were corrected on 2026-09-23; keep them as the motivating example but do not ask for those edits again. |
| validators-fail-on-empty-extraction | new | - | P3 | `other-first-party/07`, protocol-parity-21 | - | File other/07 as is. |
| hooks-dotfiles-env-unset | new | - | P3 | `other-first-party/08`, sessions-14 | - | File other/08 as is. |
| falsification-probe-first | new | - | P3 | `other-first-party/09`, sessions-12 | - | File other/09 at P3 (verifier). |
| blocked-escalation | new | - | P3 | `other-first-party/10`, sessions-13 | - | File other/10 at P3 (verifier). |
| tool-precedence-vs-harness-mode | new | - | P3 | `other-first-party/11`, sessions-16 | - | File other/11 as is. |

## Old draft file -> final bucket/slug

| Draft | Final bucket / slug |
|---|---|
| bento/00-epic-audit-2026-09-22.md | DROPPED: Replaced by the bento epic in the epic list (same title); the draft file is not filed separately. |
| bento/01-git-guard-bypass-and-false-positives.md | bento-guards-doctor-tracker / git-guard-bypasses-and-false-positives |
| bento/02-land-py-deletes-verifier-log.md | bento-landing / land-py-verifier-log-kept |
| bento/03-beads-hook-latency-budget.md | bento-guards-doctor-tracker / git-hook-latency-visibility |
| bento/04-land-work-post-push-workflow-health.md | bento-landing / land-work-post-push-workflow-health |
| bento/05-check-unpushed-overcount-and-landing-blocks.md | bento-guards-doctor-tracker / check-unpushed-overcount-and-blocks |
| bento/06-land-py-merge-abort-ownership.md | bento-landing / land-py-merge-abort-ownership |
| bento/07-land-py-invocation-progress-log.md | bento-landing / land-py-invocation-progress-log |
| bento/08-doctor-state-per-worktree.md | bento-guards-doctor-tracker / doctor-state-per-worktree |
| bento/09-stale-previews-test-leak-and-scoping.md | bento-guards-doctor-tracker / stale-previews-leak-and-scoping |
| bento/10-closure-orphan-worktree-dirs.md | bento-guards-doctor-tracker / closure-orphan-worktree-dirs |
| bento/11-landing-deletes-remote-and-superseded-branches.md | bento-landing / landing-deletes-remote-branches |
| bento/12-close-reason-evidence.md | bento-guards-doctor-tracker / close-reason-evidence |
| bento/13-claim-branch-reconciliation.md | bento-guards-doctor-tracker / claim-branch-reconciliation |
| bento/14-verifier-contract-migration.md | bento-landing / verifier-contract-migration |
| bento/15-swarm-lead-lands-from-teammate-worktree.md | bento-landing / eth-swarm-lead-lands-from-teammate |
| bento/16-land-work-skill-restructure.md | bento-landing / land-work-skill-restructure |
| bento/17-beads-snapshot-and-remote-procedure.md | bento-guards-doctor-tracker / beads-dolt-remote-guidance |
| bento/18-admission-control-hooks-and-land.md | bento-landing / dyp7-admission-control |
| bento/19-rebase-before-land-configurable.md | bento-landing / rebase-before-land-configurable |
| bento/20-merge-push-observability.md | bento-landing / merge-push-observability |
| bento/21-merge-message-and-stale-branch-nudge.md | bento-landing / merge-message-and-stale-branch-nudge |
| bento/22-followups-as-siblings.md | bento-guards-doctor-tracker / followups-as-siblings |
| bento/23-per-subagent-scratch-dirs.md | bento-guards-doctor-tracker / per-subagent-scratch-dirs |
| bento/90-note-bento-e583.md | bento-landing / e583-preview-owner-lock |
| bento/91-note-bento-a0nz.md | bento-guards-doctor-tracker / a0nz-behavioural-probe |
| other-first-party/00-dotfiles-epic.md | DROPPED: Replaced by the dotfiles epic in the epic list. |
| other-first-party/01-dotfiles-guidance-loading.md | dotfiles-global-guidance / global-guidance-actually-loads |
| other-first-party/02-dotfiles-background-wait.md | dotfiles-global-guidance / background-wait-rule-and-hook |
| other-first-party/03-dotfiles-bypass-framing.md | dotfiles-global-guidance / never-recommend-bypass |
| other-first-party/04-dotfiles-plugin-autoupdate.md | dotfiles-global-guidance / first-party-plugin-autoupdate |
| other-first-party/05-dotfiles-rtk-head-range.md | dotfiles-global-guidance / rtk-head-range-compound |
| other-first-party/06-dotfiles-memory-lifecycle.md | dotfiles-global-guidance / memory-lifecycle-rule |
| other-first-party/07-dotfiles-validator-canary.md | dotfiles-global-guidance / validators-fail-on-empty-extraction |
| other-first-party/08-dotfiles-hooks-dotfiles-env.md | dotfiles-global-guidance / hooks-dotfiles-env-unset |
| other-first-party/09-dotfiles-falsification-probe.md | dotfiles-global-guidance / falsification-probe-first |
| other-first-party/10-dotfiles-blocked-escalation.md | dotfiles-global-guidance / blocked-escalation |
| other-first-party/11-dotfiles-tool-precedence-harness.md | dotfiles-global-guidance / tool-precedence-vs-harness-mode |
| other-first-party/20-shatter-agents-epic.md | DROPPED: Replaced by the shatter-agents epic in the epic list. |
| other-first-party/21-sa-shatter-diff-nonexistent.md | shatter-agents-plugin / withdraw-shatter-diff-skill |
| other-first-party/22-sa-recipe-unimplemented.md | shatter-agents-plugin / recipes-marked-design-only |
| other-first-party/23-sa-cli-contract-test.md | shatter-agents-plugin / cli-contract-test |
| other-first-party/24-sa-delegate-to-engine.md | shatter-agents-plugin / delegate-discovery-to-engine |
| other-first-party/25-sa-note-sa-oio.md | shatter-agents-plugin / sa-oio-wrapper-convention |
| other-first-party/26-sa-wire-shatter-ci.md | shatter-agents-plugin / wire-shatter-ci-standalone |
| other-first-party/27-sa-advise-taxonomy-payload.md | shatter-agents-plugin / advise-taxonomy-payload |
| other-first-party/28-sa-claude-md-import.md | shatter-agents-plugin / claude-md-imports-agents-md |
| other-first-party/29-sa-tracker-mirrors.md | shatter-agents-plugin / close-agents-mirror-issues |
| other-first-party/30-storystore-epic.md | DROPPED: Replaced by the storystore epic in the epic list. |
| other-first-party/31-ss-migration-agents-md.md | storystore-adoption-blockers / tracker-migration-and-agents-md |
| other-first-party/32-ss-clap-cobra-extractors.md | storystore-adoption-blockers / clap-cobra-extractors |
| other-first-party/33-ss-version-bump.md | storystore-adoption-blockers / automatic-version-bump |
| other-first-party/40-bugshot-epic.md | DROPPED: Replaced by the bugshot epic in the epic list. |
| other-first-party/41-bgs-dedupe-mirrors.md | bugshot-tracker-and-payload / close-bugshot-mirror-duplicates |
| other-first-party/42-bgs-installed-cache-bloat.md | bugshot-tracker-and-payload / installed-cache-bloat-investigation |
| other-first-party/43-bgs-agents-md-structure.md | bugshot-tracker-and-payload / agents-md-structure-and-readme |
| other-first-party/50-other-effectiveness-benchmark.md | shatter-concolic-and-engine-design / effectiveness-benchmark-holdout |
| shatter-agent/01-epic-audit-2026-09-22-agent.md | DROPPED: Superseded by the single shatter epic; no per-set child epic. |
| shatter-agent/02-drift-patrol-workflow-never-runs.md | shatter-ci-workflows / drift-patrol-workflow-go-mod |
| shatter-agent/03-workflow-health-signal.md | shatter-ci-workflows / workflow-health-patrol |
| shatter-agent/04-repair-poisoned-git-identity.md | shatter-agent-guidance-and-repo-hygiene / mailmap-and-fixture-config-snapshot; shatter-agent-guidance-and-repo-hygiene / qwua7-1-git-state-check; shatter-agent-guidance-and-repo-hygiene / qwua7-51-identity-root-cause |
| shatter-agent/05-bd-sync-removed-jsonl-stale.md | shatter-tracker-and-beads / beads-jsonl-import-clobber-check; shatter-tracker-and-beads / beads-jsonl-consumers-drop-bd-sync |
| shatter-agent/06-publish-audit-reports-and-audit-skill-landing.md | shatter-tracker-and-beads / publish-audit-reports |
| shatter-agent/07-repair-stale-agent-memory.md | shatter-agent-guidance-and-repo-hygiene / repo-skills-rot; DROPPED: Moot: the shatter memory files were corrected on 2026-09-23 (maintainer, D4 note). Only the audit-skill memory-scan step survives, moved into repo-skills-rot. |
| shatter-agent/08-beads-hook-timeout-decision.md | shatter-tracker-and-beads / beads-jsonl-import-clobber-check; shatter-tracker-and-beads / beads-retire-jsonl-import-dolt-remote; shatter-tracker-and-beads / qwua7-28-superseded; DROPPED: Decision taken (D4): options (a) BEADS_HOOK_TIMEOUT env var and (b) managed hook env block are dropped; only its evidence is reused by beads-jsonl-import-clobber-check / beads-retire-jsonl-import-dolt-remote. |
| shatter-agent/09-gauntlet-scan-checker-dead.md | shatter-gates-integrity / gauntlet-scan-checker-consumes-json |
| shatter-agent/10-tracker-reconciliation-sweep.md | shatter-tracker-and-beads / tracker-reconciliation-sweep |
| shatter-agent/11-triage-policy-and-audit-epic-waves.md | shatter-tracker-and-beads / triage-policy-and-audit-epic-waves |
| shatter-agent/12-fixture-corruption-incident-and-reverify.md | shatter-agent-guidance-and-repo-hygiene / fixture-corruption-incident-reverify |
| shatter-agent/13-global-gitignore-hides-agent-config.md | shatter-agent-guidance-and-repo-hygiene / agent-config-gitignore |
| shatter-agent/14-repo-skills-rot.md | shatter-agent-guidance-and-repo-hygiene / repo-skills-rot |
| shatter-agent/15-parity-guidance-skill-and-template.md | shatter-protocol-parity / parity-guidance-skill-and-template |
| shatter-agent/16-execute-env-doctor-decisions.md | shatter-agent-guidance-and-repo-hygiene / env-doctor-decisions |
| shatter-agent/17-note-qwua7.23-rtk-block.md | shatter-agent-guidance-and-repo-hygiene / qwua7-23-agents-md-rtk-and-landing |
| shatter-agent/18-verifier-per-language-evidence.md | shatter-gates-integrity / verifier-per-language-evidence |
| shatter-agent/19-wire-every-test-module.md | shatter-gates-integrity / wire-every-test-module |
| shatter-agent/20-gate-sources-affected-completeness.md | shatter-gates-integrity / task-sources-cover-real-inputs |
| shatter-agent/21-completion-checklist-spec-docs.md | shatter-agent-guidance-and-repo-hygiene / completion-checklist-spec-docs |
| shatter-agent/22-planning-rules-location-and-open-decisions.md | shatter-agent-guidance-and-repo-hygiene / planning-rules-location-and-open-decisions |
| shatter-agent/23-mechanical-parallel-parity-gates.md | shatter-concolic-and-engine-design / engine-parity-e2e; shatter-frontend-ts / ts-branchtype-known-answer-fixtures |
| shatter-agent/24-cli-golden-output-contract-suite.md | shatter-reports-and-specs / golden-and-consumer-suite |
| shatter-agent/25-gate-golangci-lint.md | shatter-ci-workflows / go-lint-and-gofmt-gated |
| shatter-agent/26-gate-rustfmt.md | shatter-ci-workflows / rustfmt-gate |
| shatter-agent/27-downstream-coverage-goals-epic.md | shatter-tracker-and-beads / downstream-coverage-goals-epic |
| shatter-agent/28-help-tracker-ids-lint-and-unfiled-ui-items.md | shatter-cli-flags-and-help / help-tracker-ids-lint |
| shatter-agent/29-divergence-tracking-issue-liveness.md | shatter-protocol-parity / divergence-tracking-issue-liveness |
| shatter-agent/30-note-qwua7.37-premise.md | shatter-protocol-parity / qwua7-37-premise |
| shatter-agent/31-note-qwua7.43-bench.md | shatter-concolic-and-engine-design / qwua7-43-bench-dev-dep-cycle |
| shatter-code/00-epic.md | DROPPED: Superseded by the single shatter epic 'Epic: Audit 2026-09-22 findings'; every shatter issue is a child of it. |
| shatter-code/01-task-list-json-poisons-checksums.md | shatter-gates-integrity / task-list-json-poisons-checksums |
| shatter-code/02-ci-hollow-since-0829-guard-and-triage.md | shatter-gates-integrity / ci-executed-leaf-guard |
| shatter-code/03-task-sources-omit-inputs.md | shatter-gates-integrity / task-sources-cover-real-inputs |
| shatter-code/04-affected-gates-routing-gaps.md | shatter-gates-integrity / affected-gates-routing |
| shatter-code/05-go-config-discovery-unbounded.md | shatter-frontend-go / go-config-discovery-unbounded |
| shatter-code/06-ts-handlers-test-timeouts-under-load.md | shatter-test-hygiene / ts-handlers-test-timeouts |
| shatter-code/07-nextest-ci-profile-and-stale-parity-fallback.md | shatter-ci-workflows / nextest-ci-profile-and-stale-parity-fallback |
| shatter-code/08-drift-patrol-tracker-items-note.md | shatter-tracker-and-beads / qwua7-17-drift-patrol-hygiene |
| shatter-code/09-conformance-harness-correctness.md | shatter-protocol-parity / conformance-harness-correctness |
| shatter-code/10-gate-telemetry-executed-vs-cached-and-shared-cache.md | shatter-gates-integrity / gate-telemetry-executed-vs-cached |
| shatter-code/11-z3-int-real-sort-split.md | shatter-engine-correctness / z3-mixed-int-real-sort-split |
| shatter-code/12-random-explorer-path-undercount.md | shatter-engine-correctness / float-probe-paths-uncounted |
| shatter-code/13-concolic-setup-teardown-ownership.md | shatter-engine-correctness / concolic-setup-teardown |
| shatter-code/14-concolic-mock-variation-regression.md | shatter-engine-correctness / concolic-mock-variation-regression |
| shatter-code/15-duplicate-value-shrinkers.md | shatter-engine-correctness / duplicate-value-shrinkers |
| shatter-code/16-concolic-refine-phase-execute-builder.md | shatter-engine-correctness / concolic-refine-execute-builder |
| shatter-code/17-engine-path-identity-budget-config.md | shatter-concolic-and-engine-design / engine-path-identity-budget-config |
| shatter-code/18-core-dead-code-removal.md | shatter-concolic-and-engine-design / core-dead-code-removal |
| shatter-code/19-qwua7-49-rescope-note.md | shatter-engine-correctness / qwua7-49-rescope |
| shatter-code/20-invariant-min-support-templates.md | shatter-engine-correctness / invariant-min-support |
| shatter-code/21-z3-default-timeout.md | shatter-engine-correctness / z3-default-query-timeout |
| shatter-code/22-explore-resume-options-key.md | shatter-artifacts-correctness / explore-resume-options-key |
| shatter-code/23-explore-with-oracle-size-note.md | shatter-concolic-and-engine-design / qwua7-6-function-length-ratchet |
| shatter-code/24-float-constant-rational-conversion.md | shatter-engine-correctness / float-constant-rational-conversion |
| shatter-code/25-stable-hash-for-persisted-keys.md | shatter-concolic-and-engine-design / stable-hash-persisted-keys |
| shatter-code/26-explore-json-output-bundles.md | shatter-artifacts-correctness / explore-o-json-empty-bundle; shatter-artifacts-correctness / multi-file-spec-bundle-first-only |
| shatter-code/27-explore-format-flag-ignored.md | shatter-cli-flags-and-help / explore-format-flag-ignored |
| shatter-code/28-help-only-argv-intercept.md | shatter-cli-flags-and-help / help-hides-execution-flags |
| shatter-code/29-scan-artifact-filenames-abs-path.md | shatter-artifacts-correctness / scan-artifact-filenames-abs-path |
| shatter-code/30-seed-for-explore-and-run.md | shatter-cli-flags-and-help / seed-for-explore-and-run |
| shatter-code/31-unknown-config-keys-warn.md | shatter-cli-flags-and-help / unknown-config-keys-warn |
| shatter-code/32-behavior-map-cache-keys.md | shatter-artifacts-correctness / behavior-map-cache-keys |
| shatter-code/33-telemetry-known-subcommands.md | shatter-cli-flags-and-help / telemetry-known-subcommands |
| shatter-code/34-mixed-language-scan-clobbers-artifacts.md | shatter-artifacts-correctness / mixed-language-scan-deletes-artifacts |
| shatter-code/35-spec-diff-false-negative-note.md | shatter-reports-and-specs / qwua7-38-spec-diff-false-negative |
| shatter-code/36-spec-preconditions-from-path-constraints.md | shatter-reports-and-specs / spec-preconditions-from-path-constraints |
| shatter-code/37-spec-json-shapes-compare.md | shatter-reports-and-specs / spec-json-shapes-compare |
| shatter-code/38-snapshot-producer-for-diff.md | shatter-artifacts-correctness / retire-snapshot-diff |
| shatter-code/39-revalidate-ignores-return-values.md | shatter-artifacts-correctness / revalidate-return-values |
| shatter-code/40-ts-flow-map-program-point.md | shatter-frontend-ts / ts-flow-map-program-point |
| shatter-code/41-ts-switch-ternary-instrumentation.md | shatter-frontend-ts / ts-switch-ternary-instrumentation |
| shatter-code/42-ts-shadowed-callback-params.md | shatter-frontend-ts / ts-shadowed-callback-params |
| shatter-code/43-ts-timeout-classification.md | shatter-frontend-ts / ts-timeout-classification |
| shatter-code/44-ts-request-validation.md | shatter-frontend-ts / ts-request-validation |
| shatter-code/45-ts-fourth-flow-walker-note.md | shatter-frontend-ts / rf2v-fourth-walker-and-analyze-dataflow |
| shatter-code/46-ts-protocol-and-parity-tests-meaningful.md | shatter-frontend-ts / ts-protocol-and-parity-tests-meaningful |
| shatter-code/47-ts-preflight-node-modules.md | shatter-frontend-ts / ts-preflight-node-modules |
| shatter-code/48-ts-operators-collapse-to-unknown.md | shatter-frontend-ts / ts-operators-collapse-to-unknown |
| shatter-code/49-ts-lifecycle-and-packaging-hygiene.md | shatter-frontend-ts / ts-lifecycle-and-packaging-hygiene |
| shatter-code/50-go-harness-runtime-compile-path.md | shatter-frontend-go / go-harness-runtime-embed |
| shatter-code/51-go-tool-module-path.md | shatter-frontend-go / go-tool-module-path |
| shatter-code/52-go-rune-and-escape-literals.md | shatter-frontend-go / go-rune-and-escape-literals |
| shatter-code/53-go-flow-builders-note.md | shatter-frontend-go / qwua7-35-four-go-builders |
| shatter-code/54-go-dead-code-and-property-targets.md | shatter-frontend-go / go-dead-code-and-property-targets |
| shatter-code/55-go-tool-wrapper-robustness.md | shatter-frontend-go / go-tool-wrapper-robustness |
| shatter-code/56-go-small-correctness-tidy.md | shatter-frontend-go / go-small-correctness-tidy |
| shatter-code/57-go-lint-red-and-ungated.md | shatter-ci-workflows / go-lint-and-gofmt-gated |
| shatter-code/58-rust-crate-bridge-stdout.md | shatter-frontend-rust / rust-crate-bridge-stdout |
| shatter-code/59-rust-instrument-constraints.md | shatter-frontend-rust / rust-instrument-constraints |
| shatter-code/60-timeout-budget-invariant.md | shatter-frontend-rust / timeout-budget-invariant |
| shatter-code/61-rust-panic-boundary-note.md | shatter-frontend-rust / qwua7-50-panic-boundary |
| shatter-code/62-rust-main-duplicate-module-tree.md | shatter-frontend-rust / rust-main-duplicate-module-tree |
| shatter-code/63-rust-tests-offline-silent-pass.md | shatter-frontend-rust / rust-tests-offline-silent-pass |
| shatter-code/64-rust-frontend-design-dedupe.md | shatter-frontend-rust / rust-frontend-design-dedupe |
| shatter-code/65-rust-runtime-harness-loop.md | shatter-frontend-rust / rust-runtime-harness-loop |
| shatter-code/66-shatter-llm-hardening.md | shatter-frontend-rust / shatter-llm-hardening |
| shatter-code/67-parity-dispatch-reconciliation.md | shatter-protocol-parity / parity-dispatch-reconciliation |
| shatter-code/68-capability-single-source.md | shatter-protocol-parity / capability-single-source |
| shatter-code/69-validator-optional-command-warning.md | shatter-protocol-parity / validator-optional-command-warning |
| shatter-code/70-release-workflow-never-green.md | shatter-ci-workflows / release-windows-z3-build; shatter-ci-workflows / release-aarch64-openssl-cross; shatter-ci-workflows / release-publish-and-install-smoke |
| shatter-code/71-snapshot-test-helpers.md | shatter-test-hygiene / snapshot-test-helpers |
| shatter-code/72-pin-examples-repo.md | shatter-test-hygiene / pin-examples-repo |
| shatter-code/73-ci-runs-user-paths.md | shatter-ci-workflows / ci-runs-user-paths |
| shatter-code/74-collapse-test-tiers.md | shatter-test-hygiene / collapse-test-tiers |
| shatter-code/75-rapid-failfile-purge.md | shatter-test-hygiene / rapid-failfile-purge |
| shatter-code/76-broad-run-gate-duplicates.md | shatter-test-hygiene / broad-run-gate-duplicates |
| shatter-code/77-workflow-action-versions.md | shatter-ci-workflows / workflow-action-versions |
| shatter-code/78-line-coverage-metric-consistency.md | shatter-artifacts-correctness / go-scan-coverage-clamp; shatter-artifacts-correctness / rust-instrumentable-line-count |
| shatter-code/79-known-answer-ratchet-and-ts-discriminants.md | shatter-reports-and-specs / known-answer-ratchet-and-ts-discriminants |
| shatter-code/80-concolic-vs-default-benchmark.md | shatter-concolic-and-engine-design / concolic-vs-default-benchmark |
| shatter-code/81-go-explore-warmup-gate.md | shatter-frontend-go / go-explore-warmup-gate |
| shatter-code/82-rust-usize-negative-inputs.md | shatter-frontend-rust / rust-usize-negative-inputs |
| shatter-code/83-precommit-hook-fast-hermetic.md | shatter-gates-integrity / fast-hermetic-precommit |
| shatter-code/84-tests-leak-tmp-dirs.md | shatter-test-hygiene / tests-leak-tmp-dirs |
| shatter-docs-ui/01-sandbox-backend-bypasses-write-guard.md | shatter-cli-runtime-output / sandbox-backend-disables-guard |
| shatter-docs-ui/02-explore-report-underreports-paths.md | shatter-engine-correctness / float-probe-paths-uncounted |
| shatter-docs-ui/03-note-qwua7.39-json-stdout-init.md | shatter-artifacts-correctness / qwua7-39-json-stdout-first-run |
| shatter-docs-ui/04-spec-changelog-backfill-false.md | shatter-docs / spec-changelog-backfill |
| shatter-docs-ui/05-artifact-schemas-and-spec-s5.md | shatter-docs / artifact-json-schemas; shatter-docs / spec-s5-contract-table-and-samples |
| shatter-docs-ui/06-spec-s6-scan-layout-checkpoint-split.md | shatter-docs / spec-s6-layout-and-checkpoint |
| shatter-docs-ui/07-help-leaks-execution-flags.md | shatter-cli-flags-and-help / help-hides-execution-flags |
| shatter-docs-ui/08-tracker-ids-in-help-and-docs.md | shatter-cli-flags-and-help / help-tracker-ids-lint |
| shatter-docs-ui/09-scan-progress-post-hoc.md | shatter-cli-runtime-output / scan-progress-post-hoc |
| shatter-docs-ui/10-rust-runtime-path-and-doctor.md | shatter-cli-runtime-output / rust-runtime-path-and-doctor |
| shatter-docs-ui/11-per-language-outcome-rendering.md | shatter-cli-runtime-output / per-language-outcome-rendering |
| shatter-docs-ui/12-markdown-drops-render-plain-info.md | shatter-cli-runtime-output / markdown-drops-render-plain-info |
| shatter-docs-ui/13-minor-output-defects.md | shatter-cli-flags-and-help / cli-minor-output-and-help-polish |
| shatter-docs-ui/14-analyze-only-and-error-help-polish.md | shatter-cli-flags-and-help / cli-minor-output-and-help-polish |
| shatter-docs-ui/15-invariant-markdown-blank-subjects.md | shatter-reports-and-specs / invariant-markdown-blank-subjects |
| shatter-docs-ui/16-spec-yaml-custom-tags.md | shatter-reports-and-specs / spec-yaml-custom-tags |
| shatter-docs-ui/17-scan-report-headline-paths-zero-rows.md | shatter-reports-and-specs / scan-report-headline-and-paths |
| shatter-docs-ui/18-source-bucket-fixture-dir.md | shatter-reports-and-specs / source-bucket-fixture-dir |
| shatter-docs-ui/19-control-bytes-in-reports.md | shatter-reports-and-specs / control-bytes-in-reports |
| shatter-docs-ui/20-branch-metric-counts-sites.md | shatter-reports-and-specs / branch-metric-counts-sites |
| shatter-docs-ui/21-test-tier-docs-overstate-coverage.md | shatter-docs / test-tier-docs-overstate-coverage |
| shatter-docs-ui/22-crate-claude-md-stale-facts.md | shatter-docs / crate-claude-md-stale-facts |
| shatter-docs-ui/23-note-rf2v-ts-analyze-dataflow.md | shatter-frontend-ts / rf2v-fourth-walker-and-analyze-dataflow |
| shatter-docs-ui/24-protocol-schemas-reject-real-output.md | shatter-protocol-parity / protocol-schemas-reject-real-output |
| shatter-docs-ui/25-protocol-parity-md-stale.md | shatter-protocol-parity / protocol-parity-md-stale |
| shatter-docs-ui/26-protocol-rs-doc-comments.md | shatter-protocol-parity / protocol-rs-doc-comments |
| shatter-docs-ui/27-go-connection-failures-divergence.md | shatter-frontend-go / go-connection-failures-divergence |
| shatter-docs-ui/28-protocol-test-doubles-relocate.md | shatter-protocol-parity / protocol-test-doubles-relocate |
| shatter-docs-ui/29-docs-smoke-coverage.md | shatter-docs / docs-smoke-coverage |
| shatter-docs-ui/30-note-qwua7.23-agents-md.md | shatter-agent-guidance-and-repo-hygiene / qwua7-23-agents-md-rtk-and-landing |

## Finding -> final bucket/slug (all 305; P1/P2 = verifier-corrected priority)

| Finding | P (orig>verified) | Repo | Dedupe | Final bucket / slug |
|---|---|---|---|---|
| agent-repo-01 | P1>P1 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / mailmap-and-fixture-config-snapshot; shatter-agent-guidance-and-repo-hygiene / qwua7-1-git-state-check; shatter-agent-guidance-and-repo-hygiene / qwua7-51-identity-root-cause |
| agent-repo-02 | P1>P1 | shatter | related | shatter-ci-workflows / drift-patrol-workflow-go-mod |
| agent-repo-03 | P1>P1 | shatter | new | shatter-ci-workflows / workflow-health-patrol |
| agent-repo-04 | P1>P1 | shatter | partially-covered | shatter-tracker-and-beads / publish-audit-reports |
| agent-repo-05 | P2>P2 | shatter | partially-covered | shatter-tracker-and-beads / tracker-reconciliation-sweep |
| agent-repo-06 | P1>P2 | shatter | related | shatter-tracker-and-beads / beads-jsonl-consumers-drop-bd-sync; shatter-tracker-and-beads / ly5bz-superseded |
| agent-repo-07 | P2>P2 | shatter | partially-covered | shatter-tracker-and-beads / beads-jsonl-import-clobber-check; shatter-tracker-and-beads / beads-retire-jsonl-import-dolt-remote; shatter-tracker-and-beads / qwua7-28-superseded; shatter-tracker-and-beads / mpgg1-close |
| agent-repo-08 | P2>P2 | shatter | partially-covered | DROPPED: Moot: stale shatter memory entries corrected on 2026-09-23. |
| agent-repo-09 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / qwua7-23-agents-md-rtk-and-landing |
| agent-repo-10 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / agent-config-gitignore |
| agent-repo-11 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.55; linked from verifier-per-language-evidence. |
| agent-repo-12 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / qwua7-23-agents-md-rtk-and-landing |
| agent-repo-13 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.24; nothing new. |
| agent-repo-14 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / repo-skills-rot |
| agent-repo-15 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / env-doctor-decisions |
| agent-repo-16 | P2>P2 | shatter | related | shatter-agent-guidance-and-repo-hygiene / fixture-corruption-incident-reverify |
| agent-repo-17 | P3>P3 | bento | partially-covered | bento-landing / merge-message-and-stale-branch-nudge |
| agent-repo-18 | P2>P2 | shatter | partially-covered | shatter-tracker-and-beads / triage-policy-and-audit-epic-waves |
| agent-repo-19 | P3>P3 | shatter | related | shatter-agent-guidance-and-repo-hygiene / 35vtk-9-swarm-config |
| artifacts-01 | P1>P1 | shatter | related | shatter-artifacts-correctness / explore-resume-options-key |
| artifacts-02 | P1>P1 | shatter | new | shatter-artifacts-correctness / explore-o-json-empty-bundle |
| artifacts-03 | P1>P1 | shatter | related | shatter-engine-correctness / float-probe-paths-uncounted |
| artifacts-04 | P1>P1 | shatter | new | shatter-artifacts-correctness / mixed-language-scan-deletes-artifacts |
| artifacts-05 | P1>P2 | shatter | partially-covered | shatter-reports-and-specs / invariant-markdown-blank-subjects |
| artifacts-06 | P1>P2 | shatter | partially-covered | shatter-reports-and-specs / qwua7-38-spec-diff-false-negative |
| artifacts-07 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-gates-integrity / gauntlet-scan-checker-consumes-json; shatter-gates-integrity / gauntlet-checker-reopen-note |
| artifacts-08 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-reports-and-specs / spec-preconditions-from-path-constraints |
| artifacts-09 | P2>P2 | shatter | related | shatter-reports-and-specs / spec-json-shapes-compare |
| artifacts-10 | P2>P2 | shatter | new | shatter-docs / artifact-json-schemas |
| artifacts-11 | P2>P2 | shatter-agents | related | shatter-agents-plugin / withdraw-shatter-diff-skill |
| artifacts-12 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-artifacts-correctness / retire-snapshot-diff; shatter-artifacts-correctness / snapshot-diff-reopen-note |
| artifacts-13 | P2>P2 | shatter | related | shatter-reports-and-specs / scan-report-headline-and-paths |
| artifacts-14 | P2>P2 | shatter | new | shatter-reports-and-specs / spec-yaml-custom-tags |
| artifacts-15 | P2>P2 | shatter | new | shatter-reports-and-specs / scan-report-headline-and-paths |
| artifacts-16 | P3>P3 | shatter | new | shatter-cli-flags-and-help / cli-minor-output-and-help-polish |
| artifacts-17 | P1>P1 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.11; nothing new. |
| artifacts-18 | P2>P2 | shatter | related | shatter-reports-and-specs / golden-and-consumer-suite |
| bento-01 | P1>P1 | bento | duplicate-open | bento-landing / e583-preview-owner-lock |
| bento-02 | P1>P1 | bento | duplicate-closed-but-unfixed | bento-landing / land-py-verifier-log-kept; bento-landing / verifier-log-reopen-note |
| bento-03 | P1>P1 | bento | related | bento-guards-doctor-tracker / git-hook-latency-visibility |
| bento-04 | P2>P2 | bento | duplicate-closed-but-unfixed | bento-guards-doctor-tracker / git-guard-bypasses-and-false-positives; bento-guards-doctor-tracker / git-guard-reopen-note |
| bento-05 | P2>P2 | bento | new | bento-landing / land-py-merge-abort-ownership |
| bento-06 | P2>P2 | bento | new | bento-landing / land-py-invocation-progress-log |
| bento-07 | P2>P3 | bento | new | bento-landing / rebase-before-land-configurable |
| bento-08 | P2>P2 | bento | duplicate-closed-but-unfixed | bento-guards-doctor-tracker / doctor-state-per-worktree; bento-guards-doctor-tracker / doctor-state-reopen-note |
| bento-09 | P2>P2 | bento | related | bento-guards-doctor-tracker / stale-previews-leak-and-scoping |
| bento-10 | P2>P2 | bento | related | bento-guards-doctor-tracker / closure-orphan-worktree-dirs |
| bento-11 | P2>P2 | bento | related | bento-guards-doctor-tracker / check-unpushed-overcount-and-blocks |
| bento-12 | P2>P2 | bento | partially-covered | bento-landing / eth-swarm-lead-lands-from-teammate |
| bento-13 | P2>P2 | bento | partially-covered | bento-landing / verifier-contract-migration |
| bento-14 | P2>P2 | bento | partially-covered | bento-landing / land-work-skill-restructure |
| bento-15 | P2>P2 | bento | related | bento-landing / landing-deletes-remote-branches |
| bento-16 | P2>P2 | bento | related | bento-guards-doctor-tracker / beads-dolt-remote-guidance |
| bento-17 | P3>P3 | bento | related | bento-guards-doctor-tracker / claim-branch-reconciliation |
| bento-18 | P3>P3 | bento | related | bento-landing / merge-push-observability |
| cli-ux-01 | P1>P1 | shatter | related | shatter-artifacts-correctness / explore-o-json-empty-bundle |
| cli-ux-02 | P1>P2 | shatter | duplicate-closed-but-unfixed | shatter-cli-flags-and-help / explore-format-flag-ignored |
| cli-ux-03 | P1>P2 | shatter | related | shatter-cli-flags-and-help / explore-report-printed-twice |
| cli-ux-04 | P1>P1 | shatter | partially-covered | shatter-artifacts-correctness / qwua7-39-json-stdout-first-run |
| cli-ux-05 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-cli-flags-and-help / help-hides-execution-flags; shatter-cli-flags-and-help / help-flags-reopen-note |
| cli-ux-06 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-cli-runtime-output / scan-progress-post-hoc; shatter-cli-runtime-output / scan-progress-reopen-note |
| cli-ux-07 | P2>P2 | shatter | new | shatter-artifacts-correctness / scan-artifact-filenames-abs-path |
| cli-ux-08 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-cli-flags-and-help / seed-for-explore-and-run; shatter-cli-flags-and-help / seed-reopen-note |
| cli-ux-09 | P2>P2 | shatter | new | shatter-cli-flags-and-help / unknown-config-keys-warn |
| cli-ux-10 | P2>P2 | shatter | partially-covered | shatter-cli-runtime-output / rust-runtime-path-and-doctor |
| cli-ux-11 | P2>P2 | shatter | new | shatter-cli-runtime-output / per-language-outcome-rendering |
| cli-ux-12 | P2>P2 | shatter | related | shatter-artifacts-correctness / behavior-map-cache-keys |
| cli-ux-13 | P2>P2 | shatter | related | shatter-cli-runtime-output / run-report-verdict-and-coverage-metrics |
| cli-ux-14 | P2>P3 | shatter | new | shatter-cli-runtime-output / markdown-drops-render-plain-info |
| cli-ux-15 | P1>P1 | shatter | new | shatter-engine-correctness / float-probe-paths-uncounted |
| cli-ux-16 | P2>P2 | shatter | related | shatter-artifacts-correctness / explore-resume-options-key |
| cli-ux-17 | P2>P2 | shatter | related | shatter-cli-flags-and-help / help-tracker-ids-lint |
| cli-ux-18 | P3>P3 | shatter | new | shatter-cli-flags-and-help / telemetry-known-subcommands |
| cli-ux-19 | P3>P3 | shatter | partially-covered | shatter-cli-flags-and-help / cli-minor-output-and-help-polish |
| cli-ux-20 | P2>P3 | bento | new | bento-guards-doctor-tracker / per-subagent-scratch-dirs |
| cli-ux-21 | P1>P1 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.11 (P1); nothing new. Schedule .11. |
| core-01 | P1>P1 | shatter | new | shatter-engine-correctness / z3-mixed-int-real-sort-split |
| core-02 | P1>P1 | shatter | new | shatter-engine-correctness / float-probe-paths-uncounted |
| core-03 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-engine-correctness / concolic-setup-teardown; shatter-engine-correctness / setup-parity-reopen-note |
| core-04 | P1>P2 | shatter | duplicate-closed-but-unfixed | shatter-engine-correctness / concolic-mock-variation-regression; shatter-engine-correctness / mock-variation-reopen-note |
| core-05 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-engine-correctness / duplicate-value-shrinkers |
| core-06 | P2>P2 | shatter | related | shatter-engine-correctness / concolic-refine-execute-builder |
| core-07 | P2>P2 | shatter | partially-covered | shatter-concolic-and-engine-design / engine-path-identity-budget-config |
| core-08 | P2>P2 | shatter | partially-covered | shatter-concolic-and-engine-design / core-dead-code-removal |
| core-09 | P2>P2 | shatter | partially-covered | shatter-engine-correctness / concolic-setup-teardown |
| core-10 | P2>P2 | shatter | partially-covered | shatter-engine-correctness / qwua7-49-rescope |
| core-11 | P2>P2 | shatter | partially-covered | shatter-engine-correctness / invariant-min-support |
| core-12 | P2>P2 | shatter | related | shatter-engine-correctness / z3-default-query-timeout |
| core-13 | P2>P2 | shatter | related | shatter-artifacts-correctness / explore-resume-options-key |
| core-14 | P2>P2 | shatter | partially-covered | shatter-concolic-and-engine-design / engine-path-identity-budget-config |
| core-15 | P2>P3 | shatter | partially-covered | shatter-concolic-and-engine-design / qwua7-6-function-length-ratchet |
| core-16 | P2>P2 | shatter | duplicate-open | shatter-engine-correctness / concolic-refine-execute-builder; DROPPED: Duplicate-open str-qwua7.5; linked from concolic-refine-execute-builder. |
| core-17 | P3>P3 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.29/.30/.47; the array_mutation removal is in core-dead-code-removal. |
| core-18 | P3>P3 | shatter | related | shatter-reports-and-specs / branch-metric-counts-sites |
| core-19 | P3>P3 | shatter | new | shatter-engine-correctness / float-constant-rational-conversion |
| core-20 | P3>P3 | shatter | new | shatter-concolic-and-engine-design / stable-hash-persisted-keys |
| core-21 | P3>P3 | shatter | partially-covered | shatter-docs / crate-claude-md-stale-facts |
| core-22 | P2>P2 | shatter | partially-covered | shatter-concolic-and-engine-design / engine-parity-e2e |
| core-23 | P3>P3 | bento | duplicate-open | bento-guards-doctor-tracker / a0nz-behavioural-probe |
| docs-01 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-cli-runtime-output / sandbox-backend-disables-guard; shatter-cli-runtime-output / docs-first-run-reopen-note |
| docs-02 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-artifacts-correctness / retire-snapshot-diff |
| docs-03 | P2>P2 | shatter | new | shatter-reports-and-specs / spec-json-shapes-compare; shatter-docs / spec-s5-contract-table-and-samples |
| docs-04 | P2>P2 | shatter | partially-covered | shatter-tracker-and-beads / publish-audit-reports |
| docs-05 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-cli-runtime-output / docs-first-run-reopen-note; shatter-docs / spec-changelog-backfill |
| docs-06 | P2>P2 | shatter | duplicate-open | shatter-docs / wurp-changelog-row-and-reverse-check |
| docs-07 | P2>P2 | shatter | new | shatter-artifacts-correctness / mixed-language-scan-deletes-artifacts; shatter-docs / spec-s6-layout-and-checkpoint |
| docs-08 | P2>P2 | shatter | new | shatter-docs / spec-s5-contract-table-and-samples |
| docs-09 | P2>P2 | shatter-agents | partially-covered | shatter-agents-plugin / sa-oio-wrapper-convention |
| docs-10 | P2>P2 | shatter | new | shatter-agent-guidance-and-repo-hygiene / completion-checklist-spec-docs |
| docs-11 | P2>P2 | shatter | duplicate-open | shatter-docs / qwua7-52-story-seed-list |
| docs-12 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / planning-rules-location-and-open-decisions |
| docs-13 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.44/.45; nothing new. |
| docs-14 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.21.1; the schemars suggestion is minor and not a material addition. |
| docs-15 | P2>P2 | shatter | duplicate-open | shatter-artifacts-correctness / qwua7-39-json-stdout-first-run; DROPPED: Duplicate-open str-qwua7.58/.39; covered by the qwua7-39-json-stdout-first-run note. |
| docs-16 | P2>P2 | shatter | related | shatter-tracker-and-beads / beads-jsonl-import-clobber-check; shatter-tracker-and-beads / beads-jsonl-consumers-drop-bd-sync |
| docs-17 | P2>P3 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.46 (verifier P3); nothing new. |
| docs-18 | P3>P3 | shatter | partially-covered | shatter-cli-flags-and-help / help-tracker-ids-lint |
| docs-19 | P3>P3 | shatter | new | shatter-docs / docs-smoke-coverage |
| docs-20 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.57; nothing new. |
| docs-21 | P3>P3 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.61/.59; nothing new. |
| docs-22 | P3>P3 | shatter | duplicate-closed-but-unfixed | shatter-cli-flags-and-help / help-hides-execution-flags |
| docs-23 | P3>P3 | shatter | related | shatter-ci-workflows / drift-patrol-workflow-go-mod |
| docs-24 | P3>P3 | shatter | duplicate-open | shatter-docs / crate-claude-md-stale-facts; DROPPED: Duplicate-open str-qwua7.25/.23; its .js references are fixed by crate-claude-md-stale-facts. |
| frontend-go-01 | P1>P1 | shatter | new | shatter-frontend-go / go-harness-runtime-embed |
| frontend-go-02 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-frontend-go / go-tool-module-path; shatter-frontend-go / go-tool-reopen-note |
| frontend-go-03 | P1>P1 | shatter | partially-covered | shatter-frontend-go / go-rune-and-escape-literals |
| frontend-go-04 | P2>P2 | shatter | partially-covered | shatter-frontend-go / qwua7-35-four-go-builders |
| frontend-go-05 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-ci-workflows / go-lint-and-gofmt-gated |
| frontend-go-06 | P2>P2 | shatter | related | shatter-frontend-go / go-dead-code-and-property-targets |
| frontend-go-07 | P2>P2 | shatter | partially-covered | shatter-frontend-go / go-config-discovery-unbounded |
| frontend-go-08 | P2>P2 | shatter | related | shatter-frontend-go / go-build-timeout-ignored |
| frontend-go-09 | P2>P2 | shatter | partially-covered | shatter-docs / crate-claude-md-stale-facts |
| frontend-go-10 | P2>P2 | shatter | new | shatter-frontend-go / go-tool-wrapper-robustness |
| frontend-go-11 | P2>P3 | shatter | partially-covered | shatter-frontend-go / go-dead-code-and-property-targets |
| frontend-go-12 | P2>P2 | shatter | partially-covered | shatter-artifacts-correctness / explore-resume-options-key; shatter-artifacts-correctness / qwua7-39-json-stdout-first-run; shatter-reports-and-specs / branch-metric-counts-sites |
| frontend-go-13 | P3>P3 | shatter | related | shatter-frontend-go / go-cgo-refusal-covers-bodies |
| frontend-go-14 | P3>P3 | shatter | related | shatter-frontend-go / go-small-correctness-tidy |
| frontend-go-15 | P3>P3 | shatter | new | shatter-frontend-go / go-small-correctness-tidy |
| frontend-rust-01 | P1>P1 | shatter | new | shatter-frontend-rust / rust-crate-bridge-stdout |
| frontend-rust-02 | P2>P2 | shatter | partially-covered | shatter-frontend-rust / rust-instrument-constraints |
| frontend-rust-03 | P2>P2 | shatter | related | shatter-frontend-rust / rust-instrument-constraints |
| frontend-rust-04 | P2>P2 | shatter | partially-covered | shatter-frontend-rust / timeout-budget-invariant |
| frontend-rust-05 | P2>P2 | shatter | partially-covered | shatter-frontend-rust / qwua7-50-panic-boundary |
| frontend-rust-06 | P2>P2 | shatter | new | shatter-frontend-rust / rust-main-duplicate-module-tree |
| frontend-rust-07 | P2>P2 | shatter | partially-covered | shatter-gates-integrity / task-sources-cover-real-inputs |
| frontend-rust-08 | P2>P2 | shatter | partially-covered | shatter-docs / crate-claude-md-stale-facts |
| frontend-rust-09 | P2>P2 | shatter | partially-covered | shatter-concolic-and-engine-design / qwua7-43-bench-dev-dep-cycle |
| frontend-rust-10 | P2>P2 | shatter | partially-covered | shatter-tracker-and-beads / tracker-reconciliation-sweep |
| frontend-rust-11 | P2>P2 | shatter | new | shatter-frontend-rust / rust-tests-offline-silent-pass |
| frontend-rust-12 | P3>P3 | shatter | new | shatter-frontend-rust / rust-frontend-design-dedupe |
| frontend-rust-13 | P3>P3 | shatter | new | shatter-concolic-and-engine-design / stable-hash-persisted-keys |
| frontend-rust-14 | P3>P3 | shatter | new | shatter-frontend-rust / rust-frontend-design-dedupe |
| frontend-rust-15 | P3>P3 | shatter | related | shatter-frontend-rust / rust-runtime-harness-loop |
| frontend-rust-16 | P3>P3 | shatter | new | shatter-frontend-rust / shatter-llm-hardening |
| frontend-rust-17 | P3>P3 | shatter | new | shatter-frontend-rust / shatter-llm-hardening |
| frontend-rust-18 | P3>P3 | shatter | related | shatter-frontend-rust / qwua7-21-llm-seed-oracle-docs |
| frontend-ts-01 | P1>P1 | shatter | related | shatter-frontend-ts / ts-flow-map-program-point |
| frontend-ts-02 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-frontend-ts / ts-switch-ternary-instrumentation; shatter-frontend-ts / ts-branches-reopen-note |
| frontend-ts-03 | P2>P2 | shatter | partially-covered | shatter-frontend-ts / rf2v-fourth-walker-and-analyze-dataflow |
| frontend-ts-04 | P2>P2 | shatter | new | shatter-frontend-ts / ts-shadowed-callback-params |
| frontend-ts-05 | P2>P2 | shatter | new | shatter-frontend-ts / ts-timeout-classification |
| frontend-ts-06 | P2>P3 | shatter | new | shatter-frontend-ts / ts-lifecycle-and-packaging-hygiene |
| frontend-ts-07 | P2>P2 | shatter | new | shatter-frontend-ts / ts-request-validation |
| frontend-ts-08 | P2>P2 | shatter | related | shatter-frontend-ts / mhinv-3-planner-probe-not-supported |
| frontend-ts-09 | P2>P2 | shatter | partially-covered | shatter-frontend-ts / rf2v-fourth-walker-and-analyze-dataflow |
| frontend-ts-10 | P2>P2 | shatter | new | shatter-frontend-ts / ts-protocol-and-parity-tests-meaningful |
| frontend-ts-11 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-frontend-ts / ts-protocol-and-parity-tests-meaningful |
| frontend-ts-12 | P2>P2 | shatter | duplicate-open | shatter-frontend-ts / qwua7-31-eslint-evidence |
| frontend-ts-13 | P2>P3 | shatter | duplicate-closed-but-unfixed | shatter-frontend-ts / ts-preflight-node-modules |
| frontend-ts-14 | P3>P3 | shatter | related | shatter-frontend-ts / ts-operators-collapse-to-unknown |
| frontend-ts-15 | P2>P2 | shatter | partially-covered | shatter-docs / crate-claude-md-stale-facts |
| frontend-ts-16 | P3>P3 | shatter | new | shatter-frontend-ts / ts-lifecycle-and-packaging-hygiene |
| frontend-ts-17 | P3>P3 | shatter | new | shatter-frontend-ts / ts-lifecycle-and-packaging-hygiene |
| frontend-ts-18 | P2>P2 | shatter | related | shatter-frontend-ts / ts-branchtype-known-answer-fixtures |
| gates-01 | P1>P1 | shatter | partially-covered | shatter-gates-integrity / task-list-json-poisons-checksums |
| gates-02 | P1>P1 | shatter | partially-covered | shatter-gates-integrity / ci-executed-leaf-guard |
| gates-03 | P2>P2 | shatter | partially-covered | shatter-frontend-go / go-config-discovery-unbounded |
| gates-04 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate of str-6nul9, which has since landed (20692b08, merged 70465921). |
| gates-05 | P3>P3 | shatter | new | shatter-test-hygiene / ts-handlers-test-timeouts |
| gates-06 | P2>P2 | shatter | partially-covered | shatter-gates-integrity / affected-gates-routing |
| gates-07 | P2>P3 | shatter | new | shatter-test-hygiene / collapse-test-tiers; shatter-docs / test-tier-docs-overstate-coverage |
| gates-08 | P3>P3 | shatter | new | shatter-ci-workflows / nextest-ci-profile-and-stale-parity-fallback |
| gates-09 | P3>P3 | shatter | partially-covered | shatter-tracker-and-beads / qwua7-17-drift-patrol-hygiene |
| gates-10 | P3>P3 | shatter | new | shatter-protocol-parity / conformance-harness-correctness |
| gates-11 | P2>P2 | shatter | partially-covered | shatter-gates-integrity / gate-telemetry-executed-vs-cached |
| goals-01 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-artifacts-correctness / retire-snapshot-diff; shatter-artifacts-correctness / snapshot-diff-reopen-note; shatter-artifacts-correctness / diff-name-freed-note |
| goals-02 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-artifacts-correctness / revalidate-return-values; shatter-artifacts-correctness / revalidate-reopen-note |
| goals-03 | P1>P1 | shatter | new | shatter-engine-correctness / float-probe-paths-uncounted |
| goals-04 | P1>P1 | shatter | new | shatter-artifacts-correctness / multi-file-spec-bundle-first-only |
| goals-05 | P1>P1 | shatter | partially-covered | shatter-artifacts-correctness / explore-resume-options-key |
| goals-06 | P1>P1 | shatter | partially-covered | shatter-artifacts-correctness / go-scan-coverage-clamp; shatter-artifacts-correctness / rust-instrumentable-line-count |
| goals-07 | P2>P2 | shatter | partially-covered | shatter-reports-and-specs / known-answer-ratchet-and-ts-discriminants |
| goals-08 | P2>P2 | shatter | related | shatter-concolic-and-engine-design / concolic-vs-default-benchmark; shatter-concolic-and-engine-design / concolic-early-termination; shatter-concolic-and-engine-design / concolic-positioning-decision |
| goals-09 | P1>P2 | shatter | new | shatter-tracker-and-beads / downstream-coverage-goals-epic |
| goals-10 | P2>P2 | other | new | shatter-concolic-and-engine-design / effectiveness-benchmark-holdout |
| goals-11 | P2>P2 | shatter-agents | duplicate-closed-but-unfixed | shatter-agents-plugin / withdraw-shatter-diff-skill |
| goals-12 | P2>P3 | shatter | partially-covered | shatter-frontend-go / go-explore-warmup-gate |
| goals-13 | P2>P2 | shatter | related | shatter-reports-and-specs / spec-preconditions-from-path-constraints |
| goals-14 | P2>P2 | shatter | new | shatter-artifacts-correctness / behavior-map-cache-keys |
| goals-15 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-frontend-rust / rust-usize-negative-inputs; shatter-frontend-rust / rust-usize-reopen-note |
| goals-16 | P3>P3 | shatter | duplicate-closed-but-unfixed | shatter-reports-and-specs / source-bucket-fixture-dir; shatter-reports-and-specs / source-bucket-reopen-note |
| goals-17 | P3>P3 | shatter | new | shatter-reports-and-specs / control-bytes-in-reports |
| goals-18 | P3>P3 | shatter | new | shatter-cli-flags-and-help / cli-minor-output-and-help-polish |
| goals-19 | P3>P3 | shatter | duplicate-open | shatter-docs / qwua7-52-story-seed-list |
| plugins-01 | P1>P1 | shatter-agents | duplicate-closed-but-unfixed | shatter-agents-plugin / withdraw-shatter-diff-skill; shatter-agents-plugin / sa-tyb-reopen-note |
| plugins-02 | P1>P1 | shatter-agents | duplicate-closed-but-unfixed | shatter-agents-plugin / recipes-marked-design-only; shatter-agents-plugin / sa-yyt-reopen-note |
| plugins-03 | P1>P2 | shatter-agents | partially-covered | shatter-agents-plugin / cli-contract-test |
| plugins-04 | P2>P2 | dotfiles | new | dotfiles-global-guidance / first-party-plugin-autoupdate |
| plugins-05 | P1>P2 | storystore | partially-covered | storystore-adoption-blockers / clap-cobra-extractors; storystore-adoption-blockers / ss-yoa-reopen-note |
| plugins-06 | P1>P1 | dotfiles | related | dotfiles-global-guidance / global-guidance-actually-loads |
| plugins-07 | P2>P2 | storystore | new | storystore-adoption-blockers / automatic-version-bump |
| plugins-08 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / env-doctor-decisions; bugshot-tracker-and-payload / bgs-3tq-raise-to-p2 |
| plugins-09 | P2>P2 | storystore | new | storystore-adoption-blockers / tracker-migration-and-agents-md |
| plugins-10 | P2>P2 | shatter-agents | partially-covered | shatter-agents-plugin / delegate-discovery-to-engine |
| plugins-11 | P2>P2 | shatter-agents | new | shatter-agents-plugin / wire-shatter-ci-standalone |
| plugins-12 | P2>P2 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / qwua7-23-agents-md-rtk-and-landing |
| plugins-13 | P2>P2 | dotfiles | duplicate-closed-but-unfixed | dotfiles-global-guidance / rtk-head-range-compound |
| plugins-14 | P2>P2 | shatter-agents | new | shatter-agents-plugin / advise-taxonomy-payload |
| plugins-15 | P2>P2 | dotfiles | partially-covered | dotfiles-global-guidance / memory-lifecycle-rule |
| plugins-16 | P2>P2 | bugshot | new | shatter-agents-plugin / close-agents-mirror-issues; bugshot-tracker-and-payload / close-bugshot-mirror-duplicates |
| plugins-17 | P2>P2 | shatter-agents | new | shatter-agents-plugin / claude-md-imports-agents-md |
| plugins-18 | P3>P3 | bugshot | duplicate-closed-but-unfixed | bugshot-tracker-and-payload / installed-cache-bloat-investigation; bugshot-tracker-and-payload / bgs-3cz-pointer |
| plugins-19 | P3>P3 | dotfiles | new | dotfiles-global-guidance / global-guidance-actually-loads |
| plugins-20 | P3>P3 | bugshot | partially-covered | bugshot-tracker-and-payload / agents-md-structure-and-readme |
| prior-01 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-ci-workflows / drift-patrol-workflow-go-mod; shatter-ci-workflows / drift-patrol-reopen-note |
| prior-02 | P1>P1 | shatter | new | shatter-ci-workflows / release-windows-z3-build; shatter-ci-workflows / release-aarch64-openssl-cross; shatter-ci-workflows / release-publish-and-install-smoke |
| prior-03 | P1>P1 | shatter | partially-covered | shatter-tracker-and-beads / beads-jsonl-import-clobber-check; shatter-tracker-and-beads / beads-retire-jsonl-import-dolt-remote; shatter-tracker-and-beads / beads-jsonl-consumers-drop-bd-sync; shatter-tracker-and-beads / ly5bz-superseded; bento-guards-doctor-tracker / beads-dolt-remote-guidance |
| prior-04 | P1>P1 | shatter | partially-covered | shatter-agent-guidance-and-repo-hygiene / mailmap-and-fixture-config-snapshot; shatter-agent-guidance-and-repo-hygiene / qwua7-1-git-state-check |
| prior-05 | P2>P2 | shatter | partially-covered | shatter-protocol-parity / conformance-harness-correctness |
| prior-06 | P2>P2 | bento | partially-covered | bento-landing / landing-deletes-remote-branches |
| prior-07 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-cli-flags-and-help / help-hides-execution-flags; shatter-cli-flags-and-help / help-flags-reopen-note |
| prior-08 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-ci-workflows / go-lint-and-gofmt-gated; shatter-ci-workflows / go-lint-reopen-note |
| prior-09 | P2>P2 | shatter | related | shatter-tracker-and-beads / tracker-reconciliation-sweep; shatter-agent-guidance-and-repo-hygiene / qwua7-1-git-state-check |
| prior-10 | P2>P2 | bento | partially-covered | shatter-tracker-and-beads / mpgg1-close; bento-guards-doctor-tracker / claim-branch-reconciliation |
| prior-11 | P3>P3 | bento | partially-covered | bento-guards-doctor-tracker / followups-as-siblings |
| prior-12 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.62; tracker-reconciliation-sweep links it. |
| prior-13 | P2>P2 | bento | partially-covered | bento-guards-doctor-tracker / close-reason-evidence |
| prior-14 | P2>P3 | shatter | related | shatter-tracker-and-beads / triage-policy-and-audit-epic-waves |
| prior-15 | P1>P1 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.11; nothing new. |
| prior-16 | P2>P2 | shatter | duplicate-open | DROPPED: Duplicate-open str-qwua7.5 / .13 / .39; reproduced only, no new evidence (the .39 note carries the init part). |
| prior-17 | P1>P1 | shatter | new | shatter-frontend-ts / ts-switch-ternary-instrumentation |
| prior-18 | P1>P1 | shatter | new | shatter-artifacts-correctness / explore-resume-options-key |
| prior-19 | P2>P2 | shatter | new | shatter-engine-correctness / float-probe-paths-uncounted |
| prior-20 | P2>P2 | shatter | new | shatter-artifacts-correctness / rust-instrumentable-line-count |
| prior-21 | P2>P2 | shatter | duplicate-open | shatter-docs / u394l-3-pending-turns-fail |
| prior-22 | P3>P3 | shatter | new | shatter-frontend-go / go-small-correctness-tidy |
| prior-23 | P3>P3 | shatter | duplicate-closed-but-unfixed | shatter-test-hygiene / rapid-failfile-purge; shatter-test-hygiene / rapid-failfile-reopen-note |
| prior-24 | P3>P3 | shatter | partially-covered | shatter-protocol-parity / validator-optional-command-warning |
| prior-25 | P3>P3 | shatter | new | shatter-tracker-and-beads / triage-policy-and-audit-epic-waves |
| protocol-parity-01 | P1>P2 | shatter | partially-covered | shatter-protocol-parity / parity-dispatch-reconciliation |
| protocol-parity-02 | P1>P2 | shatter | new | shatter-protocol-parity / conformance-harness-correctness |
| protocol-parity-03 | P2>P2 | shatter | partially-covered | shatter-protocol-parity / protocol-schemas-reject-real-output |
| protocol-parity-04 | P2>P2 | shatter | related | shatter-gates-integrity / task-sources-cover-real-inputs |
| protocol-parity-05 | P2>P2 | shatter | related | shatter-gates-integrity / wire-every-test-module |
| protocol-parity-06 | P2>P2 | shatter | partially-covered | shatter-protocol-parity / protocol-parity-md-stale |
| protocol-parity-07 | P2>P2 | shatter | related | shatter-protocol-parity / governance-md-omits-matrix |
| protocol-parity-08 | P2>P2 | shatter | partially-covered | shatter-protocol-parity / capability-single-source |
| protocol-parity-09 | P2>P2 | shatter | related | shatter-protocol-parity / protocol-md-execute-fields |
| protocol-parity-10 | P2>P3 | shatter | new | shatter-protocol-parity / protocol-rs-doc-comments |
| protocol-parity-11 | P2>P2 | shatter | partially-covered | shatter-frontend-go / go-connection-failures-divergence |
| protocol-parity-12 | P2>P2 | shatter | new | shatter-protocol-parity / divergence-tracking-issue-liveness |
| protocol-parity-13 | P2>P2 | shatter | duplicate-open | shatter-docs / crate-claude-md-stale-facts; DROPPED: Duplicate-open str-qwua7.34/.24; its one new line is added to crate-claude-md-stale-facts. |
| protocol-parity-14 | P2>P3 | shatter | related | shatter-protocol-parity / qwua7-37-premise |
| protocol-parity-15 | P2>P2 | shatter | duplicate-open | shatter-protocol-parity / validator-ts-extraction-empty; shatter-protocol-parity / validator-reopen-note |
| protocol-parity-16 | P3>P3 | shatter | related | shatter-tracker-and-beads / tracker-reconciliation-sweep |
| protocol-parity-17 | P3>P3 | shatter | partially-covered | shatter-protocol-parity / parity-guidance-skill-and-template |
| protocol-parity-18 | P3>P3 | shatter | new | shatter-protocol-parity / parity-guidance-skill-and-template |
| protocol-parity-19 | P3>P3 | shatter | partially-covered | shatter-protocol-parity / capability-single-source |
| protocol-parity-20 | P3>P3 | shatter | new | shatter-protocol-parity / protocol-test-doubles-relocate |
| protocol-parity-21 | P3>P3 | dotfiles | new | dotfiles-global-guidance / validators-fail-on-empty-extraction |
| sessions-01 | P1>P2 | dotfiles | partially-covered | dotfiles-global-guidance / background-wait-rule-and-hook |
| sessions-02 | P1>P1 | bento | duplicate-closed-but-unfixed | bento-guards-doctor-tracker / git-guard-bypasses-and-false-positives; bento-guards-doctor-tracker / git-guard-reopen-note |
| sessions-03 | P1>P1 | shatter | partially-covered | DROPPED: Moot: shatter memory that prescribed --no-verify/hooksPath bypass was corrected on 2026-09-23. |
| sessions-04 | P1>P1 | shatter | partially-covered | shatter-gates-integrity / fast-hermetic-precommit |
| sessions-05 | P1>P1 | shatter | partially-covered | shatter-tracker-and-beads / beads-jsonl-import-clobber-check; shatter-tracker-and-beads / beads-retire-jsonl-import-dolt-remote; shatter-tracker-and-beads / qwua7-28-superseded |
| sessions-06 | P2>P2 | dotfiles | new | dotfiles-global-guidance / never-recommend-bypass |
| sessions-07 | P2>P2 | bento | partially-covered | bento-landing / dyp7-admission-control |
| sessions-08 | P2>P2 | shatter | related | shatter-test-hygiene / tests-leak-tmp-dirs |
| sessions-09 | P2>P2 | shatter | partially-covered | shatter-tracker-and-beads / tracker-reconciliation-sweep |
| sessions-10 | P2>P2 | bento | partially-covered | bento-guards-doctor-tracker / check-unpushed-overcount-and-blocks |
| sessions-11 | P2>P2 | shatter | duplicate-closed-but-unfixed | shatter-ci-workflows / rustfmt-gate; shatter-ci-workflows / rustfmt-reopen-note |
| sessions-12 | P2>P3 | dotfiles | new | dotfiles-global-guidance / falsification-probe-first |
| sessions-13 | P2>P3 | dotfiles | new | dotfiles-global-guidance / blocked-escalation |
| sessions-14 | P3>P3 | dotfiles | related | dotfiles-global-guidance / hooks-dotfiles-env-unset |
| sessions-15 | P3>P3 | bento | new | bento-guards-doctor-tracker / git-guard-bypasses-and-false-positives |
| sessions-16 | P3>P3 | dotfiles | partially-covered | dotfiles-global-guidance / tool-precedence-vs-harness-mode |
| sessions-17 | P3>P3 | bento | related | bento-guards-doctor-tracker / stale-previews-leak-and-scoping |
| tests-ci-01 | P1>P1 | shatter | partially-covered | shatter-gates-integrity / task-list-json-poisons-checksums |
| tests-ci-02 | P1>P1 | shatter | duplicate-closed-but-unfixed | shatter-ci-workflows / release-windows-z3-build; shatter-ci-workflows / release-aarch64-openssl-cross; shatter-ci-workflows / release-publish-and-install-smoke; shatter-ci-workflows / release-reopen-note |
| tests-ci-03 | P1>P1 | bento | new | shatter-ci-workflows / workflow-health-patrol; bento-landing / land-work-post-push-workflow-health |
| tests-ci-04 | P1>P1 | shatter | partially-covered | shatter-gates-integrity / task-sources-cover-real-inputs |
| tests-ci-05 | P2>P2 | shatter | new | shatter-gates-integrity / affected-gates-routing |
| tests-ci-06 | P2>P2 | shatter | partially-covered | shatter-gates-integrity / verifier-per-language-evidence |
| tests-ci-07 | P2>P3 | shatter | partially-covered | shatter-ci-workflows / nextest-ci-profile-and-stale-parity-fallback |
| tests-ci-08 | P2>P2 | shatter | new | shatter-test-hygiene / snapshot-test-helpers |
| tests-ci-09 | P2>P2 | shatter | new | shatter-test-hygiene / pin-examples-repo |
| tests-ci-10 | P2>P2 | shatter | partially-covered | shatter-gates-integrity / wire-every-test-module |
| tests-ci-11 | P2>P2 | shatter | related | shatter-ci-workflows / ci-runs-user-paths |
| tests-ci-12 | P2>P3 | shatter | related | shatter-frontend-ts / ts-protocol-and-parity-tests-meaningful |
| tests-ci-13 | P2>P3 | shatter | new | shatter-test-hygiene / collapse-test-tiers |
| tests-ci-14 | P3>P3 | shatter | related | shatter-test-hygiene / fuzz-policy-vs-reality |
| tests-ci-15 | P3>P3 | shatter | duplicate-closed-but-unfixed | shatter-test-hygiene / rapid-failfile-purge; shatter-test-hygiene / rapid-failfile-reopen-note |
| tests-ci-16 | P3>P3 | shatter | new | shatter-test-hygiene / broad-run-gate-duplicates |
| tests-ci-17 | P3>P3 | shatter | new | shatter-docs / test-tier-docs-overstate-coverage |
| tests-ci-18 | P3>P3 | shatter | new | shatter-ci-workflows / workflow-action-versions |

## Dropped or superseded (with reasons)

| Source | Reason |
|---|---|
| shatter-code/00-epic.md | Superseded by the single shatter epic 'Epic: Audit 2026-09-22 findings'; every shatter issue is a child of it. |
| shatter-agent/01-epic-audit-2026-09-22-agent.md | Superseded by the single shatter epic; no per-set child epic. |
| shatter-docs-ui/file.sh child epic 'Epic: Audit 2026-09-22 — docs accuracy and CLI/report UX' | Superseded by the single shatter epic; no nested child epic. |
| bento/00-epic-audit-2026-09-22.md | Replaced by the bento epic in the epic list (same title); the draft file is not filed separately. |
| other-first-party/00-dotfiles-epic.md | Replaced by the dotfiles epic in the epic list. |
| other-first-party/20-shatter-agents-epic.md | Replaced by the shatter-agents epic in the epic list. |
| other-first-party/30-storystore-epic.md | Replaced by the storystore epic in the epic list. |
| other-first-party/40-bugshot-epic.md | Replaced by the bugshot epic in the epic list. |
| shatter-agent/07-repair-stale-agent-memory.md | Moot: the shatter memory files were corrected on 2026-09-23 (maintainer, D4 note). Only the audit-skill memory-scan step survives, moved into repo-skills-rot. |
| shatter-agent/08-beads-hook-timeout-decision.md | Decision taken (D4): options (a) BEADS_HOOK_TIMEOUT env var and (b) managed hook env block are dropped; only its evidence is reused by beads-jsonl-import-clobber-check / beads-retire-jsonl-import-dolt-remote. |
| agent/04 step 'remove the [user] section from .git/config' | Done on 2026-09-23 (D5); the history-decision item is also settled (.mailmap, no rewrite). |
| code/70 and agent/03 'drop Windows/aarch64 from the matrix' options | D1: keep both targets and fix them. |
| code/80 criterion 'README/SPEC positioning matches measured results' | D3: measure first; positioning goes to concolic-positioning-decision, blocked by the benchmark and early-termination fix. No doc softening now. |
| code/38 option (a) 'add a snapshot producer' | D2: retire snapshot diff; spec-diff is the regression tool. |
| agent/05 decision 'keep or untrack .beads/issues.jsonl' and bento/17 'land.py exports the jsonl' option | D4: JSONL import retired and sync moves to a Dolt remote; export-only status handled in beads-jsonl-consumers-drop-bd-sync. |
| bento/03 BEADS_HOOK_TIMEOUT hydration suppression and 'env timeout' remedy | D4: no hook-timeout env var and no hook-bypass guidance. |
| all five file.sh scripts | D6: the maintainer runs one filer script after reconciliation and the Codex cross-check; agents file nothing. |
| sessions-03 | Moot: shatter memory that prescribed --no-verify/hooksPath bypass was corrected on 2026-09-23. |
| agent-repo-08 | Moot: stale shatter memory entries corrected on 2026-09-23. |
| gates-04 | Duplicate of str-6nul9, which has since landed (20692b08, merged 70465921). |
| cli-ux-21 | Duplicate-open str-qwua7.11 (P1); nothing new. Schedule .11. |
| artifacts-17 | Duplicate-open str-qwua7.11; nothing new. |
| prior-15 | Duplicate-open str-qwua7.11; nothing new. |
| prior-16 | Duplicate-open str-qwua7.5 / .13 / .39; reproduced only, no new evidence (the .39 note carries the init part). |
| core-16 | Duplicate-open str-qwua7.5; linked from concolic-refine-execute-builder. |
| core-17 | Duplicate-open str-qwua7.29/.30/.47; the array_mutation removal is in core-dead-code-removal. |
| docs-13 | Duplicate-open str-qwua7.44/.45; nothing new. |
| docs-14 | Duplicate-open str-qwua7.21.1; the schemars suggestion is minor and not a material addition. |
| docs-15 | Duplicate-open str-qwua7.58/.39; covered by the qwua7-39-json-stdout-first-run note. |
| docs-17 | Duplicate-open str-qwua7.46 (verifier P3); nothing new. |
| docs-20 | Duplicate-open str-qwua7.57; nothing new. |
| docs-21 | Duplicate-open str-qwua7.61/.59; nothing new. |
| docs-24 | Duplicate-open str-qwua7.25/.23; its .js references are fixed by crate-claude-md-stale-facts. |
| agent-repo-11 | Duplicate-open str-qwua7.55; linked from verifier-per-language-evidence. |
| agent-repo-13 | Duplicate-open str-qwua7.24; nothing new. |
| protocol-parity-13 | Duplicate-open str-qwua7.34/.24; its one new line is added to crate-claude-md-stale-facts. |
| prior-12 | Duplicate-open str-qwua7.62; tracker-reconciliation-sweep links it. |

## Filing notes for the single filer script

- File each epic first, then new issues as children of their repo's epic, then notes and reopen-notes as comments. Reopen-notes only comment on the closed issue and do not reopen it.
- Where two drafts merge, the slug's instructions name the primary draft body. Take the extra evidence from the secondary draft.
- Dependency edges named in instructions: str-qwua7.3 fix blocks ci-executed-leaf-guard. drift-patrol-workflow-go-mod blocks workflow-health-patrol. release-windows-z3-build and release-aarch64-openssl-cross block release-publish-and-install-smoke. concolic-vs-default-benchmark and concolic-early-termination block concolic-positioning-decision. beads-jsonl-import-clobber-check blocks beads-retire-jsonl-import-dolt-remote, which blocks beads-jsonl-consumers-drop-bd-sync. gauntlet-scan-checker-consumes-json blocks golden-and-consumer-suite. mixed-language-scan-deletes-artifacts blocks spec-s6-layout-and-checkpoint. artifact-json-schemas and spec-json-shapes-compare block spec-s5-contract-table-and-samples. explore-o-json-empty-bundle blocks multi-file-spec-bundle-first-only.
- Land the audit reports (publish-audit-reports) before filing, or inline the evidence, because issue bodies cite `audits/2026-09-22/` paths that exist only on branch `audit-2026-09-22`.
- The storystore tracker is write-blocked until the v32->v53 migration. Cross-repo links (for example bento git-hook-latency-visibility and shatter beads-retire-jsonl-import-dolt-remote) go in the body text, because bd cannot express them as dependencies.
- Every draft had only a local-fallback readiness check. Run bento:issue-readiness-check with a fresh reviewer on the rewritten drafts (D1-D5) and on the NEW no-draft entries before filing.
