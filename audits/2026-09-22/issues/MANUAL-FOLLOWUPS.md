# Manual follow-ups after the bulk filer run (audit 2026-09-22)

Done 2026-09-24. Input: the 74 `MANUAL follow-ups` lines printed by `file-all.sh`.
The work covered every instruction in each draft outside the posted issue body or
comment text, and checked the posted text for leftover placeholders.

Rules followed: an action was applied only when it was additive and unambiguous
(a comment, a link, a label, or a priority the draft states exactly). Closing,
reopening, retitling and description or acceptance edits are left to the maintainer.
Drafts were not edited and nothing was committed.

`$C` = `/tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/mf/c`
(the comment files; their full text is also given below). Every bd command ran in
that tracker's checkout and exited 0.

## Filer defect found

The filer's placeholder regex `<(?:id of )?slug>` does not match the
**`<slug id>`** form (for example `<retire-snapshot-diff id>`). Because of this, 10
posted comments and 4 new-issue descriptions still contained raw placeholders. A scan
of every audit comment and every audit-created issue body in the shatter, bento,
shatter-agents and bugshot trackers found all of them. Dotfiles had none. Each
one now has a follow-up comment on the same issue that resolves the ids. Descriptions
were not rewritten. Seven of these issues were **not** on the follow-up list; they are
marked "(extra)" at the end of the table. `file-all.sh` should gain the `<slug id>`
form before any rerun.

## Table

| Draft | Instruction outside the posted text | Action |
|---|---|---|
| shatter/shatter-ci-workflows/07-workflow-health-patrol.md | None. The body only says in prose that the fixes are "filed by the maintainer's filer". | No action needed (false positive). |
| shatter/shatter-ci-workflows/14-nextest-ci-profile-and-stale-parity-fallback.md | None. "Slug is kept for filer stability" is prose. | No action needed (false positive). |
| shatter/shatter-ci-workflows/16-devcontainer-workflow-red.md | None. The prose says the filer creates it, which it did. | No action needed (false positive). |
| shatter/shatter-ci-workflows/17-docker-publish-workflow-red.md | None (prose). | No action needed (false positive). |
| shatter/shatter-ci-workflows/19-ubuntu-26-runner-trial.md | None (prose). The blocked_by edge is handled by the filer. | No action needed (false positive). |
| shatter/shatter-frontend-go/07-go-dead-code-and-property-targets.md | The filer posts a companion comment on str-qwua7.48. | No action needed. The companion is posted (ledger `companion:go-dead-code-and-property-targets:str-qwua7.48`) and its `<id of …>` is substituted. |
| shatter/shatter-tracker-and-beads/07-publish-audit-reports.md | Bootstrap procedure: file this issue first, then close it before the bulk run. | No action needed. str-49drv.1 is already closed. |
| shatter/shatter-agent-guidance-and-repo-hygiene/03-qwua7-51-identity-root-cause.md | "The owner/maintainer then re-scopes the issue as the comment proposes. Do not close it." | **Maintainer decision needed:** re-scope str-qwua7.51 (BD_ACTOR→BEADS_ACTOR, run the identity probe, update bd facts). This is a description rewrite. The comment is posted and its slug placeholders were substituted. |
| shatter/shatter-agent-guidance-and-repo-hygiene/07-env-doctor-decisions.md | "Do not change its priority." The comment asks the issue owner to add `agent_env_doctor_skip_plugin=bugshot` to `.agent-mode.local`. | No tracker action. That is repo work for str-qwua7.53's owner, not a filer step. |
| shatter/shatter-agent-guidance-and-repo-hygiene/11-35vtk-9-swarm-config.md | "Do not change .9's priority." | No action needed. |
| shatter/shatter-agent-guidance-and-repo-hygiene/12-qwua7-14-reverify-on-main.md | `bd reopen str-qwua7.14` ("a manual step for the maintainer"). | **Maintainer decision needed:** reopen str-qwua7.14. It is still closed. The comment is posted. |
| shatter/shatter-agent-guidance-and-repo-hygiene/15-qwua7-52-storystore-interim-nudge.md | "Do not change its priority." The comment asks the owner to choose adopt-now or wait, and to set `remind_after`. | No tracker action. The choice is the issue owner's (see "Maintainer decisions" below). |
| shatter/shatter-artifacts-correctness/09-diff-name-freed-note.md | Replace `<retire-snapshot-diff id>` with the real id. The filer left it raw. | Applied: `bd comments add str-81xiw -f $C/str-81xiw.md` (`<retire-snapshot-diff id>` = str-49drv.15). |
| shatter/shatter-artifacts-correctness/14-qwua7-39-json-stdout-first-run.md | `bd update str-qwua7.39 --priority 1` (P2 → P1). | Applied: `bd update str-qwua7.39 --priority 1`. It was P2; it is now P1. |
| shatter/shatter-ci-workflows/04-release-reopen-note.md | "(Filer: replace the `<id of ...>` placeholders)". Do not reopen. | No action needed. The filer substituted them. |
| shatter/shatter-ci-workflows/06-drift-patrol-reopen-note.md | Replace the `<id of ...>` placeholder. | No action needed (substituted). |
| shatter/shatter-ci-workflows/09-go-lint-reopen-note.md | Replace the `<id of ...>` placeholder. | No action needed (substituted). |
| shatter/shatter-ci-workflows/11-rustfmt-reopen-note.md | Replace the `<id of ...>` placeholder. | No action needed (substituted). |
| shatter/shatter-ci-workflows/18-go-lint-qwua7-32-note.md | Replace the `<id of ...>` placeholder. | No action needed (substituted). |
| shatter/shatter-cli-flags-and-help/18-exit-codes-qwua7-12-note.md | "Do not close it", plus a Source section. | No action needed. The `<id of help-tracker-ids-lint>` placeholder was substituted. |
| shatter/shatter-cli-runtime-output/02-docs-first-run-reopen-note.md | Filing note: replace `<id of slug>` placeholders. | No action needed (substituted). |
| shatter/shatter-cli-runtime-output/04-scan-progress-reopen-note.md | Filing note: replace `<id of slug>`. | No action needed (substituted). |
| shatter/shatter-cli-runtime-output/09-doctor-rust-runtime-note.md | Filing note: replace `<id of slug>`. | No action needed (substituted). |
| shatter/shatter-cli-runtime-output/10-rust-hint-once-note.md | Filing note: replace `<id of slug>`. | No action needed (substituted). |
| shatter/shatter-concolic-and-engine-design/09-qwua7-6-function-length-ratchet.md | "Suggested priority for the target is unchanged." | No action needed. |
| shatter/shatter-concolic-and-engine-design/21-qwua7-6-2-scan-observe-config-literals.md | "Priority unchanged." | No action needed. |
| shatter/shatter-docs/08-qwua7-52-story-seed-list.md | "Do not change status or priority." | No action needed. |
| shatter/shatter-docs/09-wurp-changelog-row-and-reverse-check.md | "The maintainer may also choose to move the acceptance items into the description." | **Maintainer decision needed (optional):** whether to fold the flag-level, reverse and changelog-row checks into str-wurp's description. That is a description edit. |
| shatter/shatter-docs/11-qwua7-25-refresh-note.md | "Do not change status or priority." | No action needed. |
| shatter/shatter-engine-correctness/12-aureo-float-constant-note.md | "Propose raising the priority to P1." The draft has no `set_priority`. | **Maintainer decision needed:** raise str-aureo from P2 to P1? The draft proposes this and does not direct it, so the priority was left at P2. |
| shatter/shatter-frontend-go/03-go-tool-reopen-note.md | Leave closed. Substitute the go-tool-module-path id. | No action needed (substituted). |
| shatter/shatter-frontend-go/05-qwua7-35-four-go-builders.md | "Priority stays P2." | No action needed. The `<id of …>` placeholders were substituted. |
| shatter/shatter-frontend-rust/04-qwua7-50-panic-boundary.md | "Do not change the priority." | No action needed. |
| shatter/shatter-frontend-rust/12-qwua7-21-llm-seed-oracle-docs.md | "If the maintainer prefers a tracked child, file the Proposed child block with `--parent str-qwua7.21`." | **Maintainer decision needed (optional):** file the child issue, or keep only the comment. |
| shatter/shatter-frontend-rust/13-qwua7-36-escaping-repro.md | rust-instrument-constraints is blocked by str-qwua7.36. | No action needed. The edge str-49drv.117 → str-qwua7.36 (blocks) exists. |
| shatter/shatter-frontend-ts/07-rf2v-fourth-walker-and-analyze-dataflow.md | Substitute `<ts-flow-analysis-consolidation id>` and `<ts-flow-map-program-point id>`. The filer left them raw. | Applied: `bd comments add str-rf2v -f $C/str-rf2v.md` (= str-49drv.140, str-49drv.130). |
| shatter/shatter-frontend-ts/10-mhinv-3-planner-probe-not-supported.md | Substitute `<ts-request-validation id>`. The filer left it raw. | Applied: `bd comments add str-mhinv.3 -f $C/str-mhinv.3.md` (= str-49drv.134). |
| shatter/shatter-gates-integrity/01-task-list-json-poisons-checksums.md | (a) Add the labels quality-gates, ci, taskfile and audit if missing. (b) Add related links to str-qwua7.2 and str-35vtk.8. (c) Add an epic link if a second parent is allowed; otherwise mention the epic. (d) Replace the backticked `ci-executed-leaf-guard` and `ci-first-real-run-triage` with their ids. (e) Widen the acceptance criteria. | (a) Applied: `bd update str-qwua7.3 --add-label ci --add-label taskfile --add-label audit` (quality-gates was already present). (b) Applied: `bd link str-qwua7.3 str-qwua7.2 --type related`; `bd link str-qwua7.3 str-35vtk.8 --type related`. (c)+(d) Applied: bd allows only one parent, so `bd comments add str-qwua7.3 -f $C/str-qwua7.3.md` names str-49drv.144, str-49drv.152 and the epic str-49drv. (e) **Maintainer decision needed:** replace the acceptance with the "Widened acceptance" in the comment. That is a description/acceptance edit. |
| shatter/shatter-gates-integrity/08-gauntlet-checker-reopen-note.md | Also post a one-line comment on str-qwua7.10 citing the gauntlet-scan-checker-consumes-json id. | Applied: `bd comments add str-qwua7.10 -f $C/str-qwua7.10.md` (cites str-49drv.149). str-qwua7.10 is now **closed**. The main note's placeholder was substituted. |
| shatter/shatter-gates-integrity/11-qwua7-55-verifier-timeout-note.md | Substitute the placeholder. Add related links to str-35vtk.24 and str-qwua7.2. Add the label audit. Amend the acceptance. | The placeholder was substituted by the filer. Applied: `bd link str-qwua7.55 str-35vtk.24 --type related`; `bd link str-qwua7.55 str-qwua7.2 --type related`; `bd update str-qwua7.55 --add-label audit`. **Maintainer decision needed:** amend str-qwua7.55's acceptance as the comment shows. |
| shatter/shatter-gates-integrity/12-qwua7-2-scope-note.md | Substitute the placeholders. Add related links to str-qwua7.3, str-qwua7.55 and str-35vtk.24. Add the label audit. Amend the acceptance. | The placeholders were substituted by the filer. Applied: `bd link str-qwua7.2 str-35vtk.24 --type related`; `bd update str-qwua7.2 --add-label audit`. The links to str-qwua7.3 and str-qwua7.55 already existed from the two rows above (related, created from the other side), so they were not duplicated. **Maintainer decision needed:** amend str-qwua7.2's acceptance as the comment shows. |
| shatter/shatter-protocol-parity/11-validator-reopen-note.md | Do not reopen. Substitute the placeholders. | No action needed (substituted). |
| shatter/shatter-protocol-parity/16-qwua7-37-premise.md | "Do not change status or priority." | No action needed. |
| shatter/shatter-reports-and-specs/07-qwua7-38-spec-diff-false-negative.md | "Do not change the priority." | No action needed. |
| shatter/shatter-reports-and-specs/11-source-bucket-reopen-note.md | Do not reopen. Substitute `<source-bucket-fixture-dir id>` and `<source-bucket-config-override id>`. The filer left them raw. | Applied: `bd comments add str-9awj -f $C/str-9awj.md` (= str-49drv.182, str-49drv.186). |
| shatter/shatter-reports-and-specs/15-qwua7-10-allowlist-issue-links-note.md | "Do not change the priority." The draft assumed str-qwua7.10 was open. | No tracker action. Note: str-qwua7.10 is now **closed**, so this comment's request to widen the allowlist schema to every entry has no open owner. See the maintainer list. |
| shatter/shatter-test-hygiene/07-rapid-failfile-reopen-note.md | Do not reopen. Replace `<rapid-failfile-purge id>`. The filer left it raw. | Applied: `bd comments add str-qwua7.4 -f $C/str-qwua7.4.md` (= str-49drv.192). |
| shatter/shatter-tracker-and-beads/04-qwua7-28-superseded.md | `bd close str-qwua7.28 --reason "Superseded by <beads-retire-jsonl-import-dolt-remote id> …"`. | **Maintainer decision needed:** close str-qwua7.28 (still open). The id for the reason is **str-49drv.199**. The comment is posted and its placeholders were substituted. |
| shatter/shatter-tracker-and-beads/11-qwua7-17-drift-patrol-hygiene.md | "Post as a comment only; do not reopen." | No action needed. |
| shatter/shatter-tracker-and-beads/14-qwua7-22-audit-land-before-file-note.md | "Do not close it and do not change its priority." | No action needed. |
| bento/bento-guards-doctor-tracker/16-l01v-residual-bypasses-note.md | `set_priority: P1`; add the edge git-guard-bypasses-and-false-positives blocked-by bento-l01v. | No action needed. bento-l01v is P1, and bento-0tyd.1 is blocked by bento-l01v. |
| bento/bento-guards-doctor-tracker/17-i76i-switch-update-ref-note.md | `set_priority: P1`; add the edge git-guard-bypasses-and-false-positives blocked-by bento-i76i. | No action needed. bento-i76i is P1, and bento-0tyd.1 is blocked by bento-i76i. |
| bento/bento-guards-doctor-tracker/18-49pg-dolt-remote-section-note.md | Add the edge beads-dolt-remote-guidance blocked-by bento-49pg. | No action needed. bento-0tyd.3 is blocked by bento-49pg. |
| bento/bento-guards-doctor-tracker/19-wzbt-manual-close-note.md | Add the edge close-reason-evidence blocked-by bento-wzbt. | No action needed. bento-0tyd.7 is blocked by bento-wzbt. |
| bento/bento-landing/01-e583-preview-owner-lock.md | "Leave the priority at P1." | No action needed. |
| shatter-agents/shatter-agents-plugin/01-withdraw-shatter-diff-skill.md | The body says "the filer substitutes the id" for `<retire-snapshot-diff id>`. It did not. | Applied: `bd comments add sa-5r9.1 -f $C/sa-5r9.1.md` (= shatter str-49drv.15). |
| shatter-agents/shatter-agents-plugin/05-cli-contract-test.md | The body says the filer substitutes `<release-publish-and-install-smoke id>` and `<retire-snapshot-diff id>`. It did not. | Applied: `bd comments add sa-5r9.3 -f $C/sa-5r9.3.md` (= shatter str-49drv.23, str-49drv.15). |
| shatter-agents/shatter-agents-plugin/07-sa-oio-wrapper-convention.md | "Does not change the issue's priority." | No action needed. |
| shatter-agents/shatter-agents-plugin/13-sa-d8j-engine-discovery-note.md | "Does not change scope or priority." | No action needed. |
| bugshot/bugshot-tracker-and-payload/02-bgs-3tq-raise-to-p2.md | `bd update bgs-3tq --priority 2`. The draft has no `set_priority`, so the filer skipped it. | Applied: `bd update bgs-3tq --priority 2` (in /home/ketan/project/bugshot). It was P3; it is now P2. |
| bugshot/bugshot-tracker-and-payload/04-bgs-3cz-pointer.md | Replace `<NEW-ID>`. Do not reopen. | No action needed. `<NEW-ID>` was substituted (bgs-f6a.2). |
| dotfiles/dotfiles-global-guidance/01-global-guidance-actually-loads.md | Prose says the filer posts a slug map on the epic. | No action needed. The slug map is on #41, and #42–#53 have no unresolved `<epic>`/slug placeholders. |
| dotfiles/dotfiles-global-guidance/02-background-wait-rule-and-hook.md | Prose ("Part of #<epic>", "maintainer runs the filer"). | No action needed (substituted; false positive). |
| dotfiles/dotfiles-global-guidance/03-never-recommend-bypass.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/05-rtk-head-range-compound.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/06-memory-lifecycle-rule.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/07-validators-fail-on-empty-extraction.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/08-hooks-dotfiles-env-unset.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/09-falsification-probe-first.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/10-blocked-escalation.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/11-tool-precedence-vs-harness-mode.md | Prose. | No action needed (false positive). |
| dotfiles/dotfiles-global-guidance/12-first-party-plugin-staleness-check.md | Prose. | No action needed (false positive). |
| (extra) `revalidate-reopen-note` → str-kab3 | Unsubstituted `<revalidate-return-values id>` in the posted comment. | Applied: `bd comments add str-kab3 -f $C/str-kab3.md` (= str-49drv.14). |
| (extra) `snapshot-diff-reopen-note` → str-6k6.1 | Unsubstituted `<retire-snapshot-diff id>`. | Applied: `bd comments add str-6k6.1 -f $C/str-6k6.1.md` (= str-49drv.15). |
| (extra) `rust-usize-reopen-note` → str-ddxe | Unsubstituted `<int-unsigned64-clamp id>`. | Applied: `bd comments add str-ddxe -f $C/str-ddxe.md` (= str-49drv.154.1). |
| (extra) `ts-branches-reopen-note` → str-wsg | Unsubstituted `<ts-switch-ternary-instrumentation id>`. | Applied: `bd comments add str-wsg -f $C/str-wsg.md` (= str-49drv.131). |
| (extra) `wire-every-test-module` → str-49drv.148 (description) | `<gauntlet-scan-checker-consumes-json>` is left raw because it was filed after this issue. | Applied: `bd comments add str-49drv.148 -f $C/str-49drv.148.md` (= str-49drv.149). |
| (extra) `wire-shatter-ci-standalone` → sa-5r9.5 (description) | Unsubstituted `<release-publish-and-install-smoke id>`. | Applied: `bd comments add sa-5r9.5 -f $C/sa-5r9.5.md` (= shatter str-49drv.23). |
| (extra) `sa-tyb-reopen-note` → sa-tyb | Unsubstituted `<retire-snapshot-diff id>` and withdraw-shatter-diff-skill. | Applied: `bd comments add sa-tyb -f $C/sa-tyb.md` (= str-49drv.15, sa-5r9.1). |
| (extra) `sa-yyt-reopen-note` → sa-yyt | Unsubstituted `<recipes-marked-design-only id>`. | Applied: `bd comments add sa-yyt -f $C/sa-yyt.md` (= sa-5r9.2). |

## Maintainer decisions needed

1. **str-qwua7.51**: re-scope the identity half as the posted comment proposes (BEADS_ACTOR, identity probe, bd 1.1.0 facts).
2. **str-qwua7.14**: reopen it (`bd reopen str-qwua7.14`). The draft names this as a manual maintainer step.
3. **str-qwua7.28**: close it as superseded: `bd close str-qwua7.28 --reason "Superseded by str-49drv.199 (audit 2026-09-22, maintainer decision D4): the post-checkout stall is the JSONL import, not the timeout value."`
4. **str-qwua7.3**: replace the "explain why" acceptance with the widened acceptance in the posted comment.
5. **str-qwua7.55**: amend the acceptance (verifier timeout enforced in a way the installed runner honours) as the comment shows.
6. **str-qwua7.2**: amend the acceptance (per-leaf execution evidence; CI item moves to str-49drv.144) as the comment shows.
7. **str-aureo**: raise P2 → P1? The draft proposes this and does not direct it.
8. **str-wurp** (optional): move the flag-level, reverse and changelog-row checks from notes/comments into the description.
9. **str-qwua7.21** (optional): file the "Proposed child" LLM seed-oracle docs issue under str-qwua7.21, or keep the comment only.
10. **str-qwua7.10** is closed. The allowlist-issue-link note (and the new gauntlet one-liner) now sit on a closed issue. Either reopen it, or give the "tracker issue on every allowlist entry" requirement a new owner.
11. **file-all.sh**: extend the placeholder regex to the `<slug id>` form before any rerun.

## Comment texts posted

Placeholder resolutions (str-81xiw, str-rf2v, str-mhinv.3, str-9awj, str-qwua7.4, str-kab3,
str-6k6.1, str-ddxe, str-wsg, sa-tyb, sa-yyt) open with: "Audit 2026-09-22 placeholder
resolution: the bulk filer did not substitute the `<slug id>` placeholder form in the audit
comment above. Read it as:". They then list each placeholder with its id and title. The
description variants (sa-5r9.1, sa-5r9.3, sa-5r9.5, str-49drv.148) say "in this issue's
description" instead.

str-qwua7.3:

> Audit 2026-09-22 follow-up to the root-cause comment above (filer note from draft `task-list-json-poisons-checksums`):
> - `ci-executed-leaf-guard` = **str-49drv.144** (…) · `ci-first-real-run-triage` = **str-49drv.152** (…)
> - Both are blocked by this issue. This note belongs to the audit epic **str-49drv**; bd allows only one parent, so this issue stays under str-qwua7.

str-qwua7.10:

> Scan-failure part of the demo-gate sanity problem is filed as str-49drv.149 (audit 2026-09-22, artifacts-07).
