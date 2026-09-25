# Revision: shatter-test-hygiene (2026-09-23)

Inputs: Codex cross-check `issues/crosscheck/shatter-test-hygiene.codex.md` (primary) and the earlier degraded same-runtime review `issues/crosscheck/shatter-test-hygiene.md` (secondary). Evidence was re-verified against `origin/main` 70465921 and the audit worktree. Tracker state was checked with `bd show` for str-dl2pj (open P1), str-nl1g (open P2), str-qwua7.3 (open P1) and str-qwua7.4 (closed). A keyword scan of the 184 open shatter issues found no duplicate of any draft in this bucket. Nothing was filed.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | MAJOR | 01 says CLI snapshots are absent, but `hide_exec_flags_help.rs:47-68` has exact help fixtures | Applied. Verified; evidence now cites the existing `tests/fixtures/help/` convention. The new CLI issue extends that convention to top-level `--help`, explore and scan | 01, 10 |
| 2 | MAJOR | "Strip whitespace between tags" HTML normalizer can erase meaningful whitespace | Applied. Byte-exact is preferred. Any kept normalizer needs unit tests for inline-span spacing, `<pre>`/`<code>`/`<textarea>` and text-node whitespace. A red/green proof for markdown newline sensitivity was added | 01 |
| 3 | MAJOR | 01 and 05 bundle independently deliverable changes | Applied. 01 was split into 01 (helper fix) and new 10 `cli-output-snapshots`. 05 was split into 05 (tier set and cache identity, still blocked) and new 11 `e2e-once-in-pre-completion` (unblocked) | 01, 05, 10, 11 |
| 4 | MAJOR | Pinning the canonical checkout does not pin the snapshot (`clone --branch main`) | Applied. Verified at `examples_checkout.py:188-233` (`--branch` at :220; the cache key is the HEAD from :191). The AC now require the snapshot to be built from the SHA and verified (HEAD equals the pin, clean status) before it is returned. Tests cover fresh clone, a moved local `main` inside the refresh window, different pins, and a mismatched published dir, with red/green proof. Size S→M | 02 |
| 5 | MAJOR | Per-test env mutation of process-global `SHATTER_HARNESS_CACHE` races | Applied. Verified that `set_var` sits under `ENV_LOCK` at :11170-11321 while production readers take no lock. The AC now require an injected root or process isolation, forbid new per-test `set_var`, and require concurrency proof (two parallel `cargo test` runs plus a nextest run) | 03 |
| 6 | MINOR | 03 conflates production caches, test-only scratch and cleanup paths | Applied. `make_request_scratch` (:1091-1102) is now marked `#[cfg(test)]`. `make_harness_dir` (:1119) is noted as cleaned by `close_all`. A three-class taxonomy (intentional cache in a shared location / no cleanup / success-only cleanup) was added, with an AC to classify every site | 03 |
| 7 | MAJOR | 04 AC mandate a speculative fixture fix; `--runInBand` already used; load average isn't reproducible | Applied. Verified at `shatter-ts/Taskfile.yml:53,72`. Split into 04 (diagnosis: reproducible recipe that fails in at least 2 of 3 attempts, a profile, a recommendation) and new 12 `ts-handlers-timeout-fix` (outcome-based: 3/3 green under the recipe, red on the pre-fix commit, no unloaded regression). The maxWorkers suggestion was removed | 04, 12 |
| 8 | MAJOR | Cited logs are not at either commit | Applied. Confirmed that the logs exist only as gitignored files (`.gitignore:6 *.log`) in the audit worktree. The relevant excerpts are now inline: invocation, env, load, result lines, the 10 failing test names, and the rerun command | 04 |
| 9 | MAJOR | 05 proof (`--force` + gate-wrapper log) can't show exactly-once E2E | Applied (in 11). Cold-cache proof now needs `task --status` non-zero before the run, test-runner lines with a per-binary `grep -c` equal to 1, and a pre-fix count of 2. A wiring test locks in the exclusion. 05's `task check` proof now forces each stage directly | 05, 11 |
| 10 | MAJOR | 08 fixture migration can silently drop different assertions | Applied. Verified that the two drivers check different invariants (denominator identity + stale-source vs thresholds/ratchets). The AC now need an assertion-by-assertion table, maintainer approval for every dropped row, and red/green proof for each ported assertion class before any deletion. Size S→M | 08 |
| 11 | MAJOR | 09 "Option B plus Go -fuzztime" contradicts Option B's AC | Applied. There are now three coherent options: A (Go + cargo-fuzz), B (Go `-fuzztime` only, Rust proptest, cargo-fuzz explicitly unused) and C (no coverage-guided fuzzing). The drift check validates affirmative execution claims only, has fixture unit tests, and allows mechanisms named as unused | 09 |
| 12 | MINOR | Titles violate AGENTS.md under-50-char, area-led convention | Applied. All 12 titles are now 31-40 chars (checked by script); detail moved into bodies | all, BUNDLE.md |

## Secondary (same-runtime) review findings

| Sev | Finding | Action | Files |
|---|---|---|---|
| MAJOR | 03 undercounts fixed test `temp_dir()` paths; generated-harness capture dir omitted | Applied. Now lists 33 call sites in `mod tests` (from :7608) plus `__capture_dir` at :2525/:2704, and each must be classified. Size M→L | 03 |
| MAJOR | 05 holds the cheap E2E dedupe behind blockers | Applied (same as Codex #3) | 05, 11 |
| MINOR | Bundle header "differs only in two files" is wrong (cache.rs) | Applied | BUNDLE.md |
| MINOR | 01 CLI snapshots on the unpinned examples repo | Applied. 10 is blocked by `pin-examples-repo` | 10 |
| MINOR | 02 misses the `executor.rs:11444` fallback; `test-standard` has no `sources:` | Applied. There is an AC for the fallback, and the proof now uses `task --status workspace-test` | 02 |
| MINOR | Double E2E run happens only with cold caches | Applied in 11's problem statement | 11 |
| MINOR | 04 load recipe not reproducible | Applied (same as Codex #7) | 04 |
| MINOR | 09 drift check is extra scope | Kept, because Codex #11 relies on it to stop the policy drifting again. Bounded by fixture unit tests | 09 |

## Splits and conversions

- `snapshot-test-helpers` (01) was split. The new `cli-output-snapshots` (10) is blocked by `snapshot-test-helpers` and `pin-examples-repo`.
- `collapse-test-tiers` (05) was split. The new `e2e-once-in-pre-completion` (11) has no blockers and now owns the "E2E runs twice in pre-completion-e2e" item. 05 keeps its blockers (`task-list-json-poisons-checksums` → str-qwua7.3, `task-sources-cover-real-inputs`).
- `ts-handlers-test-timeouts` (04) is now diagnosis only. The new `ts-handlers-timeout-fix` (12) is blocked by 04.
- No slugs were removed or converted.

## Cross-bucket effects

- `shatter-docs/07-test-tier-docs-overstate-coverage.md` (:51) and `shatter-gates-integrity/04-affected-gates-routing.md` (:55) say the E2E double-run belongs to `collapse-test-tiers`. It now belongs to `e2e-once-in-pre-completion`.
- `shatter-gates-integrity/02-ci-executed-leaf-guard.md` (:29) points at `ts-handlers-test-timeouts` for the TS timeouts. That slug still exists but is now diagnosis only. The fix is `ts-handlers-timeout-fix`.
- `shatter-reports-and-specs/08` and `/09` references to `snapshot-test-helpers` and `pin-examples-repo` remain valid. CLI output snapshots are now `cli-output-snapshots`.
