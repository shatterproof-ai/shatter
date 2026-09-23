# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 85b5485da8d3047af619875b22d4d22f9d1a76048c7754516dcab74359c89a85
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-ci-workflows (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** I checked repository sources and the committed tracker snapshot. GitHub API access failed, so the live run counts and current tracker duplication remain unverified.

1. **BLOCKER — #07 makes Drift Patrol’s failure self-perpetuating.**  
   The check includes Drift Patrol itself and fails when its last three runs failed; that failure then becomes another failed patrol run. Define and test recovery from this feedback loop, or fixing every underlying defect can still leave the patrol permanently red.

2. **MAJOR — #07 omits authentication wiring for scheduled execution.**  
   `drift-patrol.yml` exports neither `GH_TOKEN` nor `GITHUB_TOKEN`; passing `repo-token` to setup-task does not authenticate later shell commands. Require explicit token wiring and a scheduled/dispatched test proving the check executes rather than returning SKIP. [GitHub CLI authentication](https://cli.github.com/manual/gh_auth_login)

3. **MAJOR — #01 understates the Windows build work.**  
   [embedded_go_frontend.rs](/home/ketan/project/shatter/shatter-cli/src/embedded_go_frontend.rs:2) unconditionally imports Unix-only `PermissionsExt` and calls `Permissions::from_mode`, and `main.rs` includes that module unconditionally. Installing Z3 cannot make this Windows build pass; include or separately track the source portability work.

4. **MAJOR — #02/#03 can approve an unusable arm64 release.**  
   [build.rs](/home/ketan/project/shatter/shatter-cli/build.rs:196) embeds a Go executable built without mapping Cargo’s target to `GOOS`/`GOARCH`; the workflow sets those variables only for the separately staged Go binary. Require an arm64 runtime smoke exercising the embedded frontend and runtime libraries, since successful cross-compilation plus x86_64 smoke does not verify that payload.

5. **MAJOR — #01/#02 recommend branch tests that can publish experimental releases.**  
   The release job has no main-branch guard, so `workflow_dispatch --ref <branch>` can publish a `continuous-*` prerelease once the matrix passes. It also omits `--target`, which defaults a newly created tag to the default branch rather than the built commit; require build-only branch validation and explicit publication targeting. [Release creation semantics](https://cli.github.com/manual/gh_release_create)

6. **MAJOR — #03 permits a smoke trigger that will not fire and alternatives #07 excludes.**  
   The current publisher uses `GITHUB_TOKEN`, whose release event will not start a downstream `release: published` workflow. Furthermore, #07 discovers only push-to-main/scheduled workflows, excluding either release-only or workflow-run-only smoke alternatives; reconcile both trigger and monitoring contracts. [GitHub trigger rules](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow)

7. **MAJOR — #08/#09 misdiagnose Task’s precondition behavior.**  
   The ordinary precondition in `shatter-go/Taskfile.yml` fails the task when golangci-lint is absent; “optional” is merely message text, not skip behavior. The actual confirmed gap is that the gate never invokes this task or installs its tool, so correct the issue and historical comments accordingly. [Task preconditions](https://taskfile.dev/docs/guide)

8. **MAJOR — #14 omits a second nextest configuration and contradicts its fallback option.**  
   Both standalone crates explicitly use [.config/nextest-standalone.toml](/home/ketan/project/shatter/.config/nextest-standalone.toml), which also contains `fail-fast = true` and an unused CI profile. Include that configuration, and make nextest-summary evidence conditional: the allowed “retain cargo test” resolution cannot supply the unconditional nextest CI URL required at close.

9. **MAJOR — #07’s filing requirements conflict with D6.**  
   D6 says the maintainer runs one filer script and no agent files anything, but #07 requires its implementer to file two new issues and close with their IDs; #13 also requires a new follow-up issue. State whether D6 applies only to initial reconciliation, or have the maintainer’s filing pass create these follow-ups beforehand.

10. **MINOR — #03’s smoke command does not test the intended installed functionality.**  
    [The `analyze` command](/home/ketan/project/shatter/shatter-cli/src/args.rs:1175) consumes saved observation JSON and explicitly requires neither frontend nor solver. Identify an actual fixture and expected result, and use an installed frontend exercise if the intended claim is that the distributed tool works end to end.

11. **MINOR — #01 misstates Z3’s feature behavior.**  
    In the cached `z3-sys 0.10.7` source, `gh-release` links Z3 statically, `bundled` builds a static library, and `static-link-z3` is a deprecated alias. Correct the proposed `gh-release`/DLL fallback so a fresh agent does not implement packaging based on the wrong linkage model.

12. **MINOR — #09 applies the wrong historical accusation to str-2tyfk.**  
    Its tracker body permits unrelated unused/govet findings to be handled separately, and its landing notes explicitly describe a scoped errcheck cleanup. Posting the same “lint acceptance was false” comment on both tickets conflates that scoped completion with str-qwua7.32’s explicit full-lint acceptance.

13. **MINOR — #05’s generic path test lacks a workable path contract.**  
    Existing workflows contain `hashFiles('**/Cargo.lock')`, which is a glob rather than a literal filesystem path. Specify glob expansion, expression handling, and exceptions for runtime-created working directories so the regression test does not reject legitimate workflows.

14. **MINOR — #12 and #14 combine independently completable work.**  
    Adding user-path CI coverage and diagnosing Perf CI are separate deliverables; adopting nextest and removing the parity-script fallback are likewise independent. Split those tasks and preserve explicit links so unrelated failures do not hold their closure together.

The three highest-value fixes are to make workflow-health recoverable and authenticated, define safe release validation with target-runtime evidence, and reconcile ownership and acceptance criteria with the actual Task/nextest behavior.
