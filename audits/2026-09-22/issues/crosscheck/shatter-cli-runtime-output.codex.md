# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** aa5bdcbf73e5ba4ee5b797a6585e9bdc2b41c672ba1026a3b2d88c21f7e6faac
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-cli-runtime-output (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

The sandbox bypass and post-hoc scan progress claims are supported by source at `56c86168`. The bundle is **not ready to file as-is**.

- **MAJOR — 01 leaves the execution policy undecided.** The acceptance criteria defer whether a Go-only sandbox setting authorizes TS/Rust execution, while other criteria assume those targets run in a throwaway directory. Choose the policy before filing, and require tests to prove target execution as well as a clean cwd; refusal or failure must not count as successful isolation.

- **MAJOR — 03’s regression test does not prove live progress.** The current post-hoc loop already prints progress before the final stdout report, satisfying the proposed ordering assertion. Require a completion event while another controlled function remains unfinished, so buffered reporting demonstrably fails.

- **MAJOR — 03’s worker-count formula does not match the scheduler.** `explore.rs:5399–5468` schedules function batches across targets, so one file target can supply multiple concurrent functions. `min(targets, workers)` can underreport concurrency; define whether the header reports configured capacity, runnable function count, or observed concurrency.

- **MAJOR — 05 misdiagnoses runtime discovery and proposes a test that can already pass.** `executor.rs:1200–1220` searches ancestors of the frontend executable, not cwd, and the Rust E2E setup explicitly supplies `SHATTER_RUNTIME_PATH`. Moving only cwd outside the repository does not reproduce a missing installation; relocate the frontend, clear the environment override, and use multiple functions to test error deduplication.

- **MAJOR — 05 duplicates tracked work without settling ownership.** Rust doctor resolution and once-only hints overlap `str-qwua7.40` and `str-qwua7.13`; the available tracker export also gives `.40` version/protocol checks and `doctor --require-rust` requirements absent here. “Coordinate or fold it in” leaves competing implementations possible; explicitly assign ownership and preserve the existing criteria when consolidating.

- **MAJOR — 05 does not define which missing prerequisites make doctor fail.** Missing Rust is a warning, but missing runtime/toolchains are subject to an undefined “blocks execution” rule. Specify applicability by selected or detected language so a TS-only installation does not fail because unused Rust or Go prerequisites are absent.

- **MAJOR — 06’s Rust `Result` heuristic can mislabel ordinary data.** The executor serializes ordinary returns through `serde_json::to_value`, and the proposed display inputs lack a reliable return-type discriminator. A normal map containing only `"Err"` or `"Ok"` is indistinguishable from the proposed envelope; require trustworthy metadata or conservative rendering, with negative tests.

- **MAJOR — 06 combines unrelated fixes.** Outcome formatting and UTF-8 safety, Rust `main` discovery exclusion, and possible mock-recording defects have separate behavior changes and verification needs. Split discovery and mock-source correctness from rendering, particularly because those observations were not re-verified.

- **MAJOR — 07 specifies two incompatible coverage contracts.** One criterion permits different metrics when explicitly named, then requires one default metric across all commands. Choose one; the proposed naming-only snapshot cannot verify metric unification.

- **MAJOR — 08’s “every fact” requirement exceeds its stated scope and tests.** Plain output also includes symbolic/MC/DC information, stubbed-import warnings, float probes, abandoned frontiers, and other conditional details. Enumerate the required parity inventory or narrow the requirement; three golden-test lines cannot justify removing plain output.

- **MAJOR — The bundle’s stderr requirements conflict.** Issue 03 requires every machine-mode stderr line to be JSON, while 01 mandates a literal warning line and 05 mandates human diagnostic text. Specify how warnings and failures are encoded in machine mode, and test these combinations.

- **MINOR — 02 incorrectly says the README has no Go-only caveat.** `README.md:309` explicitly says “(Go frontend).” The caveat is undermined by broader recommendations elsewhere, but the proposed tracker comment should describe that accurately.

- **MINOR — 06 and 08 misidentify the renderer paths.** `commands/explore.rs:3620–3648` dispatches Markdown to `render.rs`; core’s `format_exploration_report` is the legacy plain/ANSI path. Correct those references so a fresh agent edits the intended output surface.

**Verdict:** Revise before filing. The top fixes are to make regression tests discriminate the actual bugs, resolve overlapping ownership and split independent work, and settle the execution, coverage, and machine-output contracts.
