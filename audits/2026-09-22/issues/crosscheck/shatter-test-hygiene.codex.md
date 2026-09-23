# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 5a302ec8de7f2cb1558fb9ca406521773e601af4c3a767965961c22646558ee4
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-test-hygiene (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** Inspection at `70465921` found several factual errors and acceptance criteria that could permit incomplete or regressive fixes.

1. **MAJOR — 01: Existing CLI snapshots are incorrectly described as absent.** `shatter-cli/tests/hide_exec_flags_help.rs:47–68` compares complete `spec-diff` and `doctor` help output against checked-in fixtures. Narrow the claim to the missing explore/scan/top-level snapshots and explain how new snapshots fit the existing convention.

2. **MAJOR — 01: The proposed HTML normalizer can erase meaningful whitespace.** Stripping whitespace between tags changes content such as `<span>Hello</span> <span>world</span>` and can remove whitespace inside `<pre>`. Require preservation tests for inline-element spacing and preformatted content; “between tags” does not establish insignificance.

3. **MAJOR — 01 and 05: Independently deliverable changes are bundled together.** Fixing unsafe snapshot helpers does not require adding a cross-language CLI snapshot suite; removing duplicate E2E execution does not require redesigning public tiers and checksum identities. Split these deliverables so immediate correctness fixes do not depend on broader design decisions.

4. **MAJOR — 02: Pinning the canonical checkout does not guarantee pinned consumer inputs.** `examples_checkout.py:213–227` publishes snapshots using `clone --branch main`, even though the cache directory is keyed by canonical HEAD; detaching HEAD alone can therefore publish the wrong content. Require the **returned snapshot’s** HEAD/content to match the pin, covering fresh/no-update paths and callers using different pins.

5. **MAJOR — 03: Per-test environment changes can introduce isolation races.** `SHATTER_HARNESS_CACHE` is process-global, and existing tests change it while other tests read it without the same lock. Require an injected per-test root or process isolation, with concurrent verification, rather than accepting per-test environment assignment as sufficient.

6. **MINOR — 03: The evidence conflates production caches and test scratch paths.** `make_request_scratch` at `executor.rs:1081–1102` is test-only, despite being listed among production fallbacks; several fixed test directories already have successful-path cleanup, and the handler deliberately retains bridge caches. Distinguish intentional cache retention, successful-run leaks, and panic-path leaks before prescribing production fallback changes.

7. **MAJOR — 04: Acceptance mandates a speculative fix and suggests an existing concurrency limit.** The criteria require fixture restructuring even if profiling establishes CPU starvation, while both TS Task entries already invoke Jest with `--runInBand`. Make acceptance outcome-based and specify reproducible resource conditions; load average alone is not a repeatable test setup.

8. **MAJOR — 04: The cited reproduction logs are unavailable at either cited commit.** Neither `check-unit.log` nor `ts-handlers-rerun.log` exists at the stated paths in `70465921` or `56c86168`. Attach durable logs or provide retrievable artifact links and the exact invocation/environment so a fresh agent can investigate the reported failure.

9. **MAJOR — 05: The prescribed proof does not establish exactly-once E2E execution.** `Taskfile.yml:504–506` explicitly warns that outer `--force` does not propagate into nested Task processes, and the gate wrapper records outer gates rather than individual test binaries. Require verified cache invalidation and test-runner output demonstrating each intended E2E suite executed exactly once.

10. **MAJOR — 08: Fixture migration alone can silently delete regression coverage.** The runners check different invariants: `tests/scripts/broad_run_validation.py` checks denominator identities and deleted-source disappearance, while the other runner checks thresholds and source additions. Require an assertion-by-assertion comparison and preservation or explicit approval of dropped behaviors before deleting either runner.

11. **MAJOR — 09: The recommended implementation contradicts its acceptance criteria.** Option B requires saying coverage-guided fuzzing is not run, while “Option B plus” Go `-fuzztime` enables it. Choose a coherent final policy and make the drift check validate affirmative execution claims, allowing documentation to name mechanisms explicitly described as absent.

12. **MINOR — Bundle: Titles violate the repository’s issue-title convention.** The proposed titles exceed `AGENTS.md`’s under-50-character requirement and often enumerate multiple findings. Use short area-led titles and move supporting detail into the descriptions.

The top fixes are to correct the implementation and evidence claims, replace contradictory or insufficient acceptance proofs, and split the independent snapshot and gate-redesign deliverables. Issues 06/07’s file-level claims check out, but the claimed tracker status remains unverified because the available JSONL may be stale.
