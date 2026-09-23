# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Degraded same-runtime review: shatter-agents-plugin issue bundle (11 drafts)

The Codex counterpart failed identity validation (exit 4), so a Claude reviewer did this read-only review. It checked the claims against shatter-agents `119b807`, the shatter audit worktree (`target/debug/shatter`), the bd trackers in both repos, and `gh`.

## Claims verified as accurate

- 01/02: `shatter-diff/SKILL.md:46` synopsis and `:122-136` hook recipe. `shatter diff --staged` gives "unexpected argument '--staged'" (rc 2). `diff-explore` is unrecognized. `spec-diff <OLD> <NEW>` has `--json`. `args.rs:1431` `Diff { snapshot, ... }`. `plugins.json:5` lists shatter-diff. Marketplace and Codex plugin.json are both 0.1.12. sa-tyb is closed with the stated reason. str-81xiw and str-81xiw.2 are open.
- 03/04: compose-shatter-recipe lines 145, 221-239, 342 and 501 match. run-shatter `SKILL.md:69-89` "Recipe discovery and runs" matches. `run_targets.py` is 360 lines with 0 "recipe" hits. sa-yyt and sa-mty are closed with the stated reasons.
- 05: ci.yml has two jobs as described. `_is_ignored` has no status/requires handling. shatter has no refs to shatter-agents. DRIFT-PATROL `cli-surface-drift` points at str-wurp (open). `gh release list` is empty.
- 07: add-shatter-target:80/92 match. Bare `shatter` exits 2. No host-write opt-in text appears in catalog/plugins. sa-oio's body says "Do not inject --allow-...". str-gg9v is closed. str-dakf3 is open.
- 08: wire-shatter-ci install step (`curl .../main/install.sh`), run step (`python3 scripts/run_targets.py`) and the "Required companion" text match.
- 09: shatter-advise:317/319/330 match. The id grep finds exactly agents-arz and agents-2b3 in both payloads. `test_discover_hotspots.py` ships in the payload.
- 10: CLAUDE.md text is quoted exactly. bugshot CLAUDE.md is `@AGENTS.md`. AGENTS.md has the never-edit-plugins/ rule.
- 11: 67 issues (54 sa, 13 agents). 13 title-identical pairs. xj6 is in_progress on both sides. ya6 is deferred on both sides. sa-d1b is open.

## Findings

1. **MAJOR: delegate-discovery-to-engine (06) is built on a wrong model of `shatter list-targets`.** The draft says `run_targets.py` should "get target roots from `shatter list-targets --format json`" and "map the returned roots to wrappers". The real command "lists source files that would be selected for a scan". Its JSON (`kind: target_manifest`) has one `project_root` and a `selected[]` array of individual source files (`path`, `language`, `frontend`). It does not list per-package target roots (Cargo.toml / go.mod / package.json dirs), which run-shatter needs for wrapper detection. As written, the AC cannot be met without inventing a root-derivation layer. The sa-d8j regression may not be fixed by this either, because the problem there is harness Cargo.toml roots. Fix: say what the manifest actually contains. Then either (a) derive roots from `selected[].path` plus nearest manifest, with that algorithm stated, or (b) file a shatter-side request for a per-project-root listing and make this issue depend on it. Also re-scope the sa-d8j AC.
2. **MINOR: 05 says tests/ has 13 test files; there are 12** (`test_*.py`, plus the `fixtures/` directory). Correct the count or drop it.
3. **MINOR: 05 underestimates what "build shatter-cli at the pinned SHA in CI" costs.** shatter-cli needs Z3 and embeds Go/TS/Rust frontends, so a shatter-agents CI job needs those toolchains, not just `cargo build -p shatter-cli --release` plus a cache. Name the prerequisites, or allow a nightly-built binary artifact as the pin source.
4. **MINOR: 09's title and Problem say "shatter-advise/shatter-gaps cite a taxonomy spec".** Only shatter-advise cites the `docs/specs/...` path. shatter-gaps mentions "the Shatter tractability taxonomy" in its description and `<pattern_id>` in its output format, but cites no file. Reword to: shatter-advise cites an unshipped path, and shatter-gaps references the taxonomy with no resolvable source.
5. **MINOR: 07 cites `run_targets.py` line 163 for `task shatter`.** The invocation list is built at ~163 inside `detect_integration`, not in a runner function. The substance is correct. Consider citing the function names instead of line numbers, which drift.
6. **MINOR: 06 AC "enforced by a test that runs the check under `python3 -S -I`".** The config check lives in SKILL.md prose (an inline `python3 -c`), not in a script, so there is nothing for a test to execute. Either move the check into a `scripts/` helper that the test can run, or reword the AC.
7. **MINOR: 01 and 03 both require a version bump.** AGENTS.md says `scripts/build-plugins` bumps the patch version, so this is automatic when build-plugins runs. Say that, or the implementer may double-bump by hand.

## Verdict

The bundle is accurate and ready to file, except for **06**. Nearly every file, line, CLI and tracker claim checks out against the repos. The P1 pair (01/02, 03/04) is well evidenced and self-contained.

Top fixes:
1. Rewrite 06 around what `shatter list-targets` actually returns (a source-file manifest, not target roots). Re-scope its sa-d8j AC.
2. Fix 09's shatter-gaps claim and 05's test-file count.
3. Make 05's CI pin mechanism realistic about Z3 and the frontend toolchains.
