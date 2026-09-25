# Revision: shatter-agents-plugin (2026-09-23)

Applies `crosscheck/shatter-agents-plugin.codex.md` (primary) and `crosscheck/shatter-agents-plugin.md` (secondary, degraded same-runtime). Evidence re-verified against shatter-agents `119b807` and a shatter CLI built from shatter `70465921`; tracker state checked with `bd show sa-d8j`, `bd show sa-c2q`.

## Codex findings

| # | Sev | Finding | Action | Files |
|---|---|---|---|---|
| 1 | MAJOR | 06: `list-targets` returns source files, not package roots | Applied. Reproduced: on `tests/fixtures/run-targets/mixed-repo` it returns `selected: []` vs 3 roots expected at `tests/test_run_targets.py:39`. The run_targets.py delegation was dropped from 06 and turned into a note on sa-d8j (keep the local prune). | 06, 13 (new) |
| 2 | MAJOR | 06: canned-JSON test cannot prove harness exclusion; engine does not exclude `.shatter` | Applied. Reproduced: list-targets selects `.shatter/cache/harness/src/lib.rs`. Recorded in 13 and the bundle as an engine gap for the shatter tracker; 06 no longer claims to close sa-d8j. | 06, 13, BUNDLE |
| 3 | MAJOR | 05 scans `catalog/**` while 01/03 keep unreleased designs there | Applied. 05 now scans only the built payload (`plugins/claude/**`, `plugins/codex/**`), with an explicit, test-covered `cli-contract: ignore` marker for negative examples. 01 now deletes the skill (single path); 03 withdraws compose-shatter-recipe from `plugins.json` and keeps the catalog copy as a labelled design. 05 is blocked_by 01 and 03. | 01, 03, 05 |
| 4 | MAJOR | 05 help-validation cannot catch the recipe defect; bundles packaging metadata | Applied. 05 is scoped to CLI syntax and says so (and that 03's defect would not be caught); behavioural checks point to the per-skill tests. Status/requires metadata split into new 12 (P3). CI requirement made enforceable (`SHATTER_CONTRACT_REQUIRED=1` turns skip into failure). | 05, 12 (new) |
| 5 | MAJOR | 03 AC allows implementing per-recipe runs in run_targets.py | Applied. Single completion path: withdraw from payload, delete the run-shatter section, `run_targets.py` unchanged; implementation moved to Out of scope behind the engine issue. | 03, 04 |
| 6 | MAJOR | 09 path check rejects valid cross-skill refs and catches downstream paths | Applied. Check limited to bundled-resource refs (`references/`, `scripts/`, `<skill-dir>/`, `../<skill>/`) resolved against the plugin payload root; downstream/output/placeholder paths excluded; existing `shatter-gaps/SKILL.md:41` ref must pass. | 09 |
| 7 | MAJOR | 01: withdrawal leaves installed hooks broken | Applied. Added README "Removed skills" recovery text, a shatter-doctor stale-hook detector, and a git-repo test proving the hook blocks commits, the documented removal restores them, and unrelated hook logic still runs. | 01, 02 |
| 8 | MINOR | 08 omits the existing native-wrapper fallback (`SKILL.md:86-91`) | Applied. Problem restated as conflicting template/companion guidance plus insufficient Verify; AC and test now cover both the wrapper and vendored-helper paths. | 08 |
| 9 | MINOR | 09: `references/` support already exists in build-plugins | Applied. Evidence cites `build-plugins:123` and `:177-184`; suggested approach no longer proposes adding the mechanism. | 09 |
| 10 | MAJOR | Engine evidence not reproducible; cross-repo refs are draft slugs | Applied. Every engine claim now cites shatter `70465921` (args.rs identical at `16794cef`) with a rebuild command; shatter draft slugs are written as `<slug id>` placeholders the filer substitutes, with bucket names. | 01, 02, 03, 05, 06, 07, 08, 13, BUNDLE |

No Codex finding was disputed.

## Secondary (same-runtime) minors also applied

- 05 test-file count corrected to 12 `test_*.py` plus `fixtures/`; build prerequisites (Z3, Go, Node) named, with a CI-artifact pin alternative.
- 09 title and Problem: only shatter-advise cites the spec path; shatter-gaps cites no source.
- 07 cites function names (`package_manager_command`, `detect_integration`, `execute_targets`) alongside drifting line numbers.
- 06 config check moved into a `scripts/check_config.py` helper so the `python3 -I -S` test has something to run.
- 01/03/09 say the patch version bump is automatic via `scripts/build-plugins` (AGENTS.md), not a hand edit.

## Splits and conversions

- **06 delegate-discovery-to-engine** narrowed to shatter-doctor (report `shatter doctor`, tested config helper). Slug kept for stability; title changed. The run_targets.py half became **13 sa-d8j-engine-discovery-note** (note-to-existing on open sa-d8j, verified with `bd show`). sa-c2q is unaffected.
- **05 cli-contract-test** split: metadata/build-plugins mechanism became **12 skill-status-metadata** (P3). 05 now blocked_by withdraw-shatter-diff-skill and recipes-marked-design-only.
- No slugs removed.

## Engine-side gaps surfaced (for the shatter tracker, not filed here)

- `shatter list-targets` selects sources under `.shatter/cache/harness/` (its walker lacks the `.shatter` exclusion that `GLOB_WALK_EXCLUDE_DIRS` has).
- No shatter command rejects a malformed `.shatter/config.yaml` (`list-targets` exits 0 on `foo: [unclosed`), and `shatter doctor` does not parse it.
