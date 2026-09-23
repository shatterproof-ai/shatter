# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback; Codex failed identity validation)

Artifact: audits/2026-09-22/issues/shatter/shatter-agent-guidance-and-repo-hygiene/BUNDLE.md (11 drafts)
Checked read-only against /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (origin/main 70465921), /home/ketan/project/shatter (bd 1.1.0), /home/ketan/project/bento, /home/ketan/dotfiles.

## Claims verified as accurate
- No `.mailmap`; history shows 285 `Test` / 961 `Test User` <test@example.com> lines; since 2026-06-24 origin/main has 123 Test + 455 Test User vs 6 real authored commits. No repo-local `user.*`; `core.bare=false`.
- `scripts/test_git_fixture_isolation.py` ENTRYPOINTS (shell + Python only), snapshot of the caller repo only; `Taskfile.yml:418/450` wiring.
- `e50fc399` (2026-09-07, Test, "init", 82 files, +620/-12090) is NOT an ancestor of origin/main; contained in exactly the 4 named `origin/str-qwua7.*` branches; both `recovery/*` branches exist; str-qwua7.14 is CLOSED with the quoted reason.
- `.gitignore:130/134/165`, 17 tracked `.claude` files, `.claude/worktrees/str-umw3` exists.
- Skill lines: check-go:10, audit:72 (`GLOSSARY.md`; only `docs/GLOSSARY.md` exists), :151, :153, :385 (`bd sync`), bugfix:29-31; no `CLI_ARGS` in any Taskfile; bd 1.1.0.
- `.agent-mode.local` content; doctor recognised keys at `agent-env-doctor.py:69,72`; the five orphan dirs exist.
- AGENTS.md:528/531/533/535/594, :118-125, :370-375, :513; sizes 30,731 + 8,617 = 39,348 bytes.
- CLAUDE.md:43-53 checklist, :57 batch-landing claim, :80; SPEC.md:3 "Last updated: 2026-09-09", :7 changelog rule, §2 at :68, §8 at :1173; grep for SPEC/changelog/QUICKSTART/stories in CLAUDE.md + pre-completion finds nothing.
- Plan file: 1,694 lines, 43 unticked boxes, no `Status:`; `:1037` quoted; `Cargo.toml:44`; both test files use shatter_llm. swarm-discover.py:18-22 reads only JSON configs; check-all `disable-model-invocation: true`.
- All referenced bd issues exist with the stated status/priority (qwua7.1/.23/.43/.44/.51/.52/.53 open; .14 and hjrnp.4 closed; 35vtk.9 open P1; u394l.4 open).

## Findings

### mailmap-and-fixture-config-snapshot
- **MAJOR - Whole-file hash of the shared `.git/config` will flake under concurrent sessions.** AC3 fails the test if `$(git rev-parse --git-common-dir)/config` changes at all during the run, but that file is shared by every worktree and is legitimately rewritten by other agents (`branch.<name>.*` entries from `push -u`/tracking, worktree config, remotes; the primary currently holds 2 `branch.*` entries). In a swarm, `task meta` would fail spuriously. Scope the guard to the damage class (`user.*`, `core.bare`, `core.hooksPath`, `core.worktree`, and any key newly added by an ENTRYPOINT's own identity values), or compare a filtered key set, and say so in the AC.
- **MINOR - AC4 probe design is under-specified.** Pointing `ROOT` at a temp clone changes which ENTRYPOINTS paths resolve; the suggested approach (parameterise the check function) should be the required form, otherwise the proof may exercise a different code path than the real run.
- **MINOR - "every commit ... from about 2026-06-23" is slightly overstated**: 6 real-identity commits exist on origin/main since then. Harmless, but "nearly every" is accurate.

### qwua7-1-git-state-check
- **MINOR - Effective-identity condition (2) will FAIL legitimately inside fixture/test repos** if the check is ever run from one; the SKIP rule covers CI and non-worktrees only. State that it inspects the checkout the drift patrol is invoked from, not arbitrary cwd.

### qwua7-51-identity-root-cause
- **MINOR - Root cause is asserted, then hedged.** "bd falls back to git user.name ... consistent with that" is inference; the proposed fresh-claim probe is the right verification, so frame the comment's cause as probable until that probe runs.

### fixture-corruption-incident-reverify
- **MINOR - Bundles three tasks** (incident record, branch review/deletion, str-qwua7.14 re-diagnosis) plus an AGENTS.md rule, while str-qwua7.23 is actively cutting AGENTS.md. Acceptable as one incident issue, but AC5 should coordinate with str-qwua7.23's byte budget (or land in the land-work/close-reason guidance instead).
- **MINOR - str-qwua7.14's close reason says it rebuilt from HEAD**, so "diagnosis ran against a corrupted tree" is likely but not proven; AC4 correctly requires re-diagnosis, the Problem text should say "may have".

### agent-config-gitignore
- **MINOR - AC1 leaves `.codex/` rules open-ended** ("any other codex files the project intends to track"); the primary's `.codex/skills` and `.codex/swarm-config.md` symlinks will surface as untracked once `.codex/` is un-ignored. State explicitly whether those symlinks are tracked or ignored.

### repo-skills-rot
- **MINOR - Scope is wide** (4 skill deletions/rewrites, audit-skill fixes, a memory-contradiction step, bugfix edits, and a new lint with bd-subcommand validation). Consider splitting the lint (AC6-7) into its own issue or folding it into str-u394l.4 as the draft already half-suggests.
- **MINOR - `blocked_by` names a slug from another bucket** (`beads-retire-jsonl-import-dolt-remote`); the filer must resolve it to a real id, and the block applies only to AC5, which bd cannot express. Consider "related" instead of a hard block.

### env-doctor-decisions
- No significant issues. (Doctor line numbers :537/:997-999 not re-verified.)

### qwua7-23-agents-md-rtk-and-landing
- **MINOR - Merged-branch counts drifted**: now 37 merged of 64 remote refs (includes origin/HEAD), not 35 of 66. Say "about" or re-run at filing.

### completion-checklist-spec-docs
- **MINOR - AC2's waiver mechanism is vague**: "completion message carries an explicit waiver line" is not something a diff-based check can read. Specify where the waiver lives (commit trailer, e.g. `Spec-Waiver: <reason>`) so the check is mechanical.

### planning-rules-location-and-open-decisions
- **MINOR - "Plan told the implementer to add shatter-llm under dev-dependencies" overstates the contradiction**: that dev-dependency already existed since 2026-05-25 (4db63be3), and the plan line itself notes it is dev-only. The real conflict is adding a second consumer (`bench_frontier_ranking.rs`) of an edge str-qwua7.43 is removing; reword the Problem accordingly.

### 35vtk-9-swarm-config
- No significant issues; claims verified.

## Verdict
Ready to file after small edits. No BLOCKERs. One MAJOR: draft 01's whole-file `.git/config` hash will produce false failures in a shared, concurrently-used checkout. Top fixes:
1. Draft 01: narrow the real-config guard to identity/core keys (or a filtered key diff) rather than a whole-file digest.
2. Draft 10: correct the dev-dependency framing (pre-existing edge; new consumer is the conflict).
3. Draft 09: make the SPEC waiver a machine-readable commit trailer; draft 06: consider splitting the lint out.
