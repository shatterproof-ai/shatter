# Area review: user and contributor documentation (L2, L3)

Audit 2026-09-22, worktree `audit-2026-09-22` (based on main `16794cef`).
Reviewer scope: README, QUICKSTART, CONTRIBUTING, SPEC.md, PLAN.md, GLOSSARY,
`docs/` tree and `docs/INDEX.md`, per-crate CLAUDE.md, docs/stories, and the
agent-system mechanisms that let doc drift appear or linger.

Method: read the docs; compared SPEC §2 against `--help` for all 24 commands
(scripted, 220 per-command long flags plus 12 global flags); ran the CLI on
small TS fixtures in the session scratchpad to check SPEC §5/§6 output formats,
exit codes, and the host-write policy; ran a link/anchor checker and an orphan
checker over all 114 tracked `.md` files; read tracker state with `bd` (the
committed `.beads/issues.jsonl` is 15 days stale; see D-16).

Binary provenance: CLI runs used `/home/ketan/project/shatter/target/debug/shatter`
(built 2026-09-22 10:33 from main; no `args.rs`/`main.rs` changes after that
point except already-merged commits). A worktree build was started in parallel
(`cargo build -p shatter-cli`) under `run-heavy`. Load was very high (load
average 60-200), so that build was still compiling when this report was written.

Prior audit reference: `audit-2026-09-04/audits/2026-09-04/{docs-accuracy,docs-quality}.md`.

---

## 1. Status of 2026-09-04 documentation findings

| Prior finding | Tracker | Status now | Evidence |
|---|---|---|---|
| QUICKSTART first run refused by default-deny | str-qwua7.8 (closed 09-14) | **Fixed** | QUICKSTART.md:78-96 and README.md:262-269 pass `--allow-host-writes` and explain the policy |
| SPEC lacks `--allow-host-writes` / sandbox policy | str-qwua7.8 | **Fixed** | SPEC.md:588 (§2.10 row), :591-624 (new subsection), §2.11 exit-2 row |
| SPEC §3.6 `inputs:` list does not deserialize | str-qwua7.8/.9 | **Fixed** | SPEC.md:727 now `inputs: ./inputs/validateToken/candidates.json` |
| SPEC §5.5 snapshot example wrong | str-qwua7.9.2 | **Fixed** | SPEC.md:866-883 has `"version": 1`, `created_at` |
| docs-smoke only checks flag names | str-qwua7.9 | **Fixed** | `scripts/docs-smoke.yaml` executes `shatter explore --allow-host-writes {fixtures}/shipping.ts:calculateShipping`; typed blocks are deserialized into `ShatterConfig`/`Snapshot` (CONTRIBUTING.md:126-135) |
| SPEC changelog stale since 07-03 | str-qwua7.8 | **Partly fixed** | Rows backfilled (SPEC.md:1177-1182), but some rows point to sections that were never updated (D-05). Rows are also missing for the 09-14 and 09-19 edits (D-05) |
| SPEC §2.11 cites `--failure-threshold` | (in .8 facts, str-wurp notes) | **Still wrong** | SPEC.md:634. `args.rs` has no such flag |
| SPEC §2.8/§2.9 omit init .gitignore, doctor `-d`/gitignore/config report | str-qwua7.8 facts | **Still wrong** | SPEC.md:475-493, :532-536 (D-05) |
| README `shatter.config.json` table missing 5 fields | str-qwua7.21.1 (open) | Still true | README.md:177-192 vs `config.rs:482-585` |
| §6.2 checkpoint `config_hash` optional; `--spec-out` bundle not shown | — | Still true, and worse (D-07, D-08) | |
| PARITY.md stale / CI-INTEGRATION stale / ERRORS.md stub | str-qwua7.45/.44 (open) | Still true | PARITY.md:7 "Last updated: 2026-05-13", :69-70, :84. docs/CI-INTEGRATION.md:10-12, :327-333. ERRORS.md is 9 bytes |
| INDEX misframes PLAN; CLAUDE.md sends agents to PLAN for architecture | str-qwua7.45 | Still true | docs/INDEX.md:15, CLAUDE.md:5 and :88 vs PLAN.md:3 |
| 27 orphan docs, no status convention, duplicate plan/spec/audit trees | str-qwua7.44 (open) | **Worse**: 40+ orphans; new plan landed in the duplicate tree (D-12) | orphan script output below |
| Doc lint gates silently skip | str-qwua7.46 (open) | Still true | `audits/2026-09-22/gates/check.log`: `[skip] markdownlint-cli2 not installed`, `[skip] vale…`, `[skip] lychee…` |
| AGENTS.md rtk block wraps project section; 39 KB auto-load | str-qwua7.23 (open) | Still true | AGENTS.md:528 marker, :535 `## Shared-Machine Resource Etiquette`, :594 close; 30,731 B |
| Crate CLAUDE.md oversized | str-qwua7.25 (open) | Unchanged | go 54,640 B; ts 31,989 B; rust 21,672 B |
| `shatter-ts/CLAUDE.md` cites `.js` files | not filed | Still true | shatter-ts/CLAUDE.md:379-380 |
| GLOSSARY narrow; terminology | str-qwua7.57 (open) | Still true (D-20) | docs/GLOSSARY.md is 56 lines and unchanged |
| docs/stories absent | str-qwua7.52 (open), str-u394l.3 | Still absent 16 days after the "adopt" decision (D-11) | `ls docs/stories` gives ENOENT |

Net: the P1 first-run items were fixed well, and docs-smoke hardening was
done properly. The IA/structure items (`.21`, `.44`, `.45`, `.46`, `.52`) have
not moved. The class fix for SPEC↔clap drift (`str-wurp`) is still open after
three months.

---

## 2. Positives worth preserving

- **SPEC flag coverage is now close to complete.** A scripted diff of the help
  output for every command against SPEC.md found **all 220 per-command long flags and all 12 global flags**
  mentioned. Checked per section, only `doctor --directory` and
  `list-targets --scope` are missing from their own sections. `scan` delegates
  25 explore-shared flags to "shared with `explore`", and all 25 exist on
  `explore`. Documented defaults match the help text for every table row checked
  (the script flagged only rows where the help states the default in prose,
  and each of those matched by hand).
- **The docs-smoke gate is effective now.** It executes QUICKSTART's example
  shape for real and deserializes typed config and snapshot blocks into the
  real structs. The skip directive requires a reason. I ran it with an
  extended doc list (resource-parameters, distribution, execution-adapters,
  PROJECT-LAYOUT) and it passed. Those docs are healthy, just unguarded (D-18).
- **Link hygiene is excellent.** A relative-link and anchor checker over all
  114 tracked `.md` files found 0 broken links. The only 2 hits are `...`
  placeholders in shatter-go/CLAUDE.md. Backticked repo paths in 26 core docs
  all resolve.
- **The first-run story now holds.** QUICKSTART §2 runs as written. Exit
  codes for the error paths I tried (refusal, bad spec, missing snapshot,
  usage error, missing target) are all `2`, which matches §2.11.
- PLAN.md and execution-adapters.md carry clear "historical / not current"
  banners. That is the right pattern to generalise.

---

## 3. Findings (new first, then confirmations)

### D-01 [P1, L2/L4] The "recommended" sandbox remedy removes all write protection for TS and Rust targets

- README.md:309-310 ("Recommended: run targets inside an OS sandbox (Go
  frontend). `export SHATTER_SANDBOX_BACKEND=docker`") and QUICKSTART.md:83-85
  tell users with a **TypeScript** example to use "`export
  SHATTER_SANDBOX_BACKEND=docker` once per shell instead". SPEC.md:604-606 and
  the refusal message (`host_writes.rs:94-110`, "Configure an OS sandbox
  (recommended)") say the same.
- Code: `SHATTER_SANDBOX_BACKEND` is read only by the Go frontend
  (`shatter-go/sandbox/runner.go:16`; `grep -rn SHATTER_SANDBOX_BACKEND` finds no
  TS or Rust reader). `host_writes.rs:142-145`: when the variable is set to
  anything other than `none`, `setup()` returns `Ok(None)` and **skips the
  throwaway-directory guard**.
- Reproduction (scratchpad `sbx/w.ts`, a TS function that calls
  `fs.writeFileSync("marker-long.txt")`/`("marker-short.txt")`):
  - `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch --max-iterations 5`
    exits 0, and `marker-long.txt` and `marker-short.txt` are **created in the invoking directory**.
  - The control run, `shatter explore w.ts:touch --max-iterations 5 --allow-host-writes`,
    leaves no marker files.
- Impact: the docs call this the safest option, but for 2 of 3 frontends it
  disables both default-deny and the throwaway directory. This recreates the
  stray-file incident that str-gg9v was created to prevent.
- Tracker: not tracked. `SANDBOX_BACKEND` appears in 23 issues, and none records
  this.

### D-02 [P1, L5/L2] `shatter diff` is documented as the snapshot-regression workflow, but no command writes a snapshot

- Docs: README.md:338 ("`shatter diff` and `shatter spec-diff`: compare current
  behavior against a saved baseline"); QUICKSTART.md:160 (`shatter diff
  snapshots/shipping.json current/shipping.json`); SPEC §3.2:669-670
  ("Behavior maps are serialized as snapshots for `diff`"), §5.5 ("Consumed by
  `shatter diff`").
- Code: `Snapshot::from_behavior_map(s)` (`shatter-core/src/snapshot.rs:69-87`)
  and `write_to_file` have **no non-test callers**. A workspace grep finds only
  `snapshot.rs:603` (a test) plus the `read_from_file` calls in
  `shatter-cli/src/commands/diff.rs:17,23`. No flag in any `--help` output
  mentions writing a snapshot.
- Reproduction: `shatter diff` fails with exit 2 on every JSON the CLI
  produces:
  - on `.shatter-cache/behavior-maps/classifyNumber.json`: "missing field `version`"
  - on the `--spec-out` bundle: "missing field `function_id`"
  - on `shatter-artifacts/explore-results/c.ts/00001_classifyNumber.json`:
    "missing field `created_at`"
- The producer was specified in str-6k6.1 ("Implement snapshot export…"),
  which is closed, but the export path no longer exists.

### D-03 [P2, L2/L3] SPEC does not say which command produces which JSON artifact, and producers and consumers disagree

- `explore --spec-out s.json` writes a `FileSpecBundle` (`{version, file,
  functions[]}`, observed). SPEC §5.3 shows only a bare `FunctionSpec`, and the
  word "bundle" never appears in SPEC.md except for bench and TS.
- `shatter spec-diff s.json s.json` accepts the bundle (exit 0), but
  `shatter compare s.json s.json` rejects it: "missing field `function_name`",
  exit 2. SPEC §2.6 says `compare` takes "two spec JSON files" and does not
  say that only `specify --json` output works.
- `explore --spec-json` to stdout emits the markdown report followed by JSON
  (observed: `stdout-spec.json` starts with `# Shatter Explore`). This is
  str-qwua7.11 (open), and SPEC §2.1 still presents `--spec-json` as
  machine-readable output.
- A contract table would have exposed D-02 and this finding: for each artifact
  (observation, analysis, solve, FunctionSpec, FileSpecBundle, Snapshot,
  behavior map, scan manifest, checkpoint), list the producing command/flag,
  the consuming commands, and the schema.

### D-04 [P2, AGENT] The previous audit report was never landed; 68 open issues cite evidence on a local-only branch

- `git log main..audit-2026-09-04` shows 7 commits (report, evidence, triage
  drafts, decisions). `git ls-remote origin 'refs/heads/audit*'` lists only
  `audit-kapow-2026-05-24`. `git ls-tree main -- audits` contains only the
  02-28 and 05-21 reports.
- 68 non-closed tracker issues reference `audits/2026-09-04…` paths in their
  bodies. Those paths do not exist on main or origin, so any other clone, CI,
  or cloud agent cannot follow them.
- `.claude/skills/audit/SKILL.md:9,383-385` says to write
  `audits/YYYY-MM-DD.md` and "commit report", but has no step to land or push
  the audit branch. It also writes to root `audits/`, while str-qwua7.44
  decided to merge audits into `docs/audits/`. This audit (09-22) repeats the
  pattern.
- docs/INDEX.md has no Audits section, so even landed audits (02-28, 05-21)
  are orphans.

### D-05 [P2, L2] SPEC changelog backfill claims section updates that never happened; newest CLI-visible edits have no rows

- SPEC.md:1178: "2026-08-10 | `shatter init`/`shatter doctor` gitignore every
  generated path … | 2.8, 2.9". But §2.8 (SPEC.md:475-493) still says init
  creates only `.shatter/` and `config.yaml`, and §2.9 `doctor` (SPEC.md:532-536)
  still describes only embedded-frontend staleness. `init --help` also omits
  .gitignore. `doctor --help` documents the `.gitignore` check and `-d/--directory`,
  and `shatter doctor` also prints a "Project configuration" report (observed).
- SPEC.md:1181: "2026-07-18 … two-file config split … | 2.9, 3.6". §3.6
  (SPEC.md:715-748) never mentions `shatter.config.json`.
- The header says "Last updated: 2026-09-09" (SPEC.md:3). SPEC.md was edited
  again on 09-14 (`21981b1d`, `c8bceb32`) and 09-19 (`2de05fd9` str-nfg4y,
  which changed spec-diff help and SPEC text). Neither edit has a changelog
  row, and neither does str-qwua7.15 (09-14, which changed `--help` output for
  five commands).
- Root cause: str-qwua7.8 closed with "Closed" and no reason, after adding
  rows but not the content those rows describe.

### D-06 [P2, L2 + AGENT] Flag-level SPEC drift keeps recurring because the class fix (str-wurp) has been open for three months

- Still present: SPEC.md:634 `scan --fail-on-failures`/`--failure-threshold`
  (`--failure-threshold` does not exist). §2.9 `doctor` lacks `-d/--directory`,
  and `list-targets` lacks `--scope` (`list-targets --help`: "Path to a scope
  configuration YAML file").
- `str-wurp` (mechanical CLI-surface drift gate) was opened 2026-06-12 and is
  still open. docs/DRIFT-PATROL.md:41 marks `cli-surface-drift` as "not
  implemented". The 30-line script in §4 of this report reproduces the check.

### D-07 [P2, L2] SPEC §6.1/§6.2 live-output and artifact layout are stale, and the code splits one scan across two directories

- §6.1 example events show `"function":"calculateShipping"` and a field table
  of 6 fields. `shatter scan . --progress` emits
  `"function":"/abs/path/c.ts::classifyNumber","qualified_id":…,"display_name":"classifyNumber"`.
  The two extra fields are undocumented, and `function` is an absolute path
  plus `::name`.
- §6.2 says `<scan-id-prefix>` is "the first 16 hex characters of a SHA-256
  hash computed from the sorted list of source file paths". Code:
  `checkpoint.rs:132-147` (`scan_id_v2`) hashes `(qualified_id, source_file)`
  pairs. The checkpoint is written to `scan-results/<first 16>/checkpoint.json`
  (`checkpoint.rs:197-203`, hardcoded `shatter-artifacts`), but every other scan
  artifact goes to `scan-results/<full 64>/` (`scan_orchestrator.rs:556-564`,
  via `resolve_artifact_root`). Observed directory:
  `scan-results/4dfd2b95…36f8/{manifest.json,run-status.json,run-status.tsv,summary.json,functions/}`.
  SPEC documents none of these files.
- One scan therefore writes to two sibling directories, and the checkpoint
  ignores a configured artifact root. This is also an L1 defect; the code
  reviewer should confirm it.

### D-08 [P2, L2] SPEC §5.1 "Exploration Report (default)" shows a format the CLI no longer prints

- SPEC.md:795-804 shows `Explored: classifyNumber / Iterations: 50 / Unique paths: 4 / New paths: [1] …`.
- Actual default output (observed) is the termimad/markdown report:
  `# Shatter Explore` / ``## `classifyNumber` *(c.ts:1-6)*`` /
  `**4 path(s)** · **100%** coverage (4/4 lines)` / a `| # | Call | Outcome |`
  table / `**Summary:** …`.
- docs-smoke cannot catch this because the block has no language tag.
  Snapshot tests exist for scan markdown (`shatter-core/tests/snapshots/*.md`),
  and SPEC could embed or link one of those instead of hand-written samples.

### D-09 [P2, L2, cross-repo] The documented build-tool wrapper convention runs bare `shatter`, which exits 2

- README.md:233-240 documents a Makefile target of `$(SHATTER_BIN)` with no
  subcommand and no host-write opt-in. The shatter-agents skill
  `add-shatter-target` (source `catalog/skills/add-shatter-target/SKILL.md:80`:
  "The wrapper command itself should be `shatter` (no extra flags)") writes
  `"shatter": "shatter"` / `cmds: [shatter]`, and `run-shatter`'s
  `run_targets.py` invokes `task shatter` / `npm run shatter` with no args.
- Bare `shatter` prints usage and exits 2 (observed).
- Real consumers (kapow `Makefile:931`, zolem `Makefile:29-30`) replace the
  wrapper with scripts, which suggests the default was never exercised
  end-to-end.
- shatter-agents does not mention `--allow-host-writes`,
  `SHATTER_ALLOW_HOST_WRITES`, or the sandbox anywhere (grep). It was last
  committed 2026-07-28, 19 days after the default-deny policy landed. The
  installed plugin cache is `shatterproof/shatter/0.1.1`, but the source repo
  says it bumped to `0.1.10` (commit `5e5ec73`), so installed agent guidance
  is also stale.

### D-10 [P2, AGENT] The completion checklist never asks for SPEC, QUICKSTART, or changelog updates

- `grep -n 'SPEC.md\|changelog' CLAUDE.md .claude/skills/pre-completion/SKILL.md`
  returns nothing. The only doc rule is CLAUDE.md:80, "Update README.md when
  build/run/config procedures change".
- SPEC.md:7 asks for a changelog row on every CLI-visible change, but nothing
  in the landing path reads or enforces that rule. D-05 and D-06 are the
  result.
- No skill or checklist mentions `storystore:stories-impact-check`, even though
  it is installed as a hard trigger for user-facing behavior changes. It is a
  no-op here because there are no stories (D-11).

### D-11 [P2, L3 + AGENT] No intent/story documentation 16 days after the adopt decision; proposed initial story set

- `docs/stories/` is absent. str-qwua7.52 recorded "Adopt storystore" on
  2026-09-06 and is still open with no activity. str-u394l.3 has been open
  since 2026-07-06. docs/DRIFT-PATROL.md:42 still lists `docs-stories` as "not
  implemented".
- A reference implementation exists at `/home/ketan/project/bento/docs/stories/`
  (20 stories + INDEX + README). storystore's default coverage surface is
  `cli-command`, and Shatter has 24.
- Recommended seed set (observed mode first; promote to `accepted` after
  maintainer interview). Each story lists its evidence:
  1. **First exploration of one function** (explore; QUICKSTART §2; docs-smoke
     smoke command; e2e_concolic.rs).
  2. **Safe execution of untrusted targets** (default-deny, `--allow-host-writes`,
     sandbox backends; host_writes.rs tests). This story would have caught D-01.
  3. **Repository scan in CI with failure thresholds** (scan/run,
     `--fail-on-failures`, coverage-budget gates, exit codes; gauntlet).
  4. **Behavioral baseline and regression check in a PR** (spec-out → spec-diff
     / stale / diff). Writing this story would have exposed D-02 and D-03.
  5. **Resume an interrupted scan** (§6; checkpoint tests). Would have caught D-07.
  6. **Functions that need live resources** (setup/generators/opaque types;
     docs/resource-parameters.md).
  7. **Offline staged pipeline** (observe→analyze→solve→specify).
  8. **Cross-language equivalence check** (compare).
  9. **Project init, config and doctor** (init/doctor/two config files/implicit
     init; D-05, D-15).
  10. **Agent-driven usage through the shatter-agents plugin** (run-shatter,
      add-shatter-target, interpret-spec; D-09).
  11. **Reviewing results in the HTML report** (scan -o report.html; open HTML
      issues str-6prkb, str-8uomd, str-77803).
- Also: link `docs/stories/INDEX.md` from docs/INDEX.md, and land str-u394l.3
  so the drift-patrol slot becomes real.

### D-12 [P2, AGENT] The superpowers default plan path keeps growing the duplicate plan tree

- A new plan landed on 09-21 at
  `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
  (1,695 lines). It is an orphan (linked from nowhere), has no Status line, and
  its checkboxes are still unticked after the work landed (str-hjrnp.4).
- Cause: `superpowers:writing-plans` SKILL.md:18 says "Save plans to:
  `docs/superpowers/plans/…` (User preferences for plan location override this
  default)". Neither CLAUDE.md nor AGENTS.md states a plan/spec location, so
  str-qwua7.44's "merge the duplicate trees" decision is being undone as new
  plans are written.
- Status banners (ad hoc): 21 plan/spec files, of which 16 have no `Status:`.
  `docs/plans/vscode-extension-design.md` describes `shatter-vs`, which was
  deleted on 09-06 (str-qwua7.41), and has no banner.

### D-13 [P2, L3] IA debt from 09-04 unchanged, with more orphans (confirms str-qwua7.44/.45)

- An orphan scan (md files not linked from any other md) lists 40 files under
  docs/, audits/, and the repo root, including ERRORS.md, LANGUAGE-EVALUATION.md,
  PARITY.md, docs/performance-profiling.md, docs/perf/{efficiency-plan,
  gate-budgets, gate-overlap-audit, gate-receipt-v1, ws-*}.md, 11 docs/plans,
  3 docs/specs, 7 docs/superpowers, docs/validation/{2026-04-go-frontend-kapow-rerun,
  broad-run-corpus}.md, and the 2 root audits. `docs/perf/gate-budgets.md` and
  `gate-receipt-v1.md` are live contributor references, not archive material.
- docs/INDEX.md:15 still says PLAN.md "describes planned/in-progress work".
  CLAUDE.md:5 says "See `PLAN.md` for architecture". CONTRIBUTING.md:60-70
  project structure omits `protocol/`, `scripts/`, `shatter-llm/`,
  `shatter-go-tool/`, `shatter-rust-runtime/`, and `perf/`.

### D-14 [P2, L2] Config reference gap (confirms str-qwua7.21.1)

- README.md:177-192 lists 14 `shatter.config.json` fields. `ProjectConfig`
  (`config.rs:482-558`) also has `parallelism_min`, `parallelism_max`,
  `observer_pool`, `candidate_queue_capacity`, and `coverage_budget_gates{6
  sub-fields}`. `scan --help` tells users these can be set in
  `shatter.config.json`.
- These `.shatter/config.yaml` keys appear in **no** user doc (README, SPEC,
  PROJECT-LAYOUT, resource-parameters): `mock_fixtures`, `fuzz`,
  `session_setup`, `file_setup` (`config.rs:155-178,883-936`).
- PROJECT-LAYOUT.md:59-62 says "See the README for the full schema reference".
  README.md:200 says "See docs/PROJECT-LAYOUT.md for the full schema". Each
  doc defers to the other, and neither has the full schema.

### D-15 [P2, L2/L6] Implicit init contradicts README's "separate steps" and pollutes stdout (confirms str-qwua7.58/.39)

- README.md:124-127 says "Installing the `shatter` binary and initializing a
  project are separate steps". QUICKSTART §3 presents init as optional.
- Observed: the first `shatter explore c.ts:classifyNumber > report.md` in an
  empty dir writes `Created .shatter/`, `Created .shatter/config.yaml (detected
  language: unknown)`, and `Created .gitignore (4 generated path(s) ignored)`
  **into report.md** (stdout). It also modifies the user's `.gitignore`.
  Only PROJECT-LAYOUT.md:40 hints at this: "some commands will initialize it
  implicitly".
- The generated `.shatter/config.yaml` header says "See the 'Project
  Configuration' section of README.md". In a user's repo that points at *their*
  README. It should be a URL.

### D-16 [P2, AGENT] The committed tracker snapshot is 15 days stale, and CI drift-patrol reads it

- `git log -1 -- .beads/issues.jsonl` is 2026-09-07. In the JSONL, str-qwua7.8,
  .9, .15, .56, .4, and .7 are `open`. In `bd show` they are closed (09-08 to
  09-22).
- docs/DRIFT-PATROL.md says tracker data comes "from the committed
  `.beads/issues.jsonl` export — which is why the check works in CI". CI's
  `tracker-hygiene` therefore reports against two-week-old state. Anyone
  reading the repo (or an agent without `bd`) sees fixed doc issues as open.
- Related: str-ly5bz (bd sync cadence), str-qwua7.62.

### D-17 [P2, AGENT] Doc lint still skips in the landing gate (confirms str-qwua7.46)

- `audits/2026-09-22/gates/check.log`: `[skip] markdownlint-cli2 not
  installed`, `[skip] vale not installed`, `[skip] lychee not installed`.
  CI does not install them either (`grep` of `.github/workflows` finds no
  install).

### D-18 [P3, L2] docs-smoke guards only 4 docs

- `scripts/docs-smoke.yaml` `docs:` = README, QUICKSTART, SPEC, docs/INDEX.
  docs/resource-parameters.md (4 checkable blocks), docs/distribution.md (4),
  docs/execution-adapters.md (3), and PROTOCOL.md (22 JSON blocks) are not
  checked. An extended run passes today, so adding them costs nothing now.

### D-19 [P3, L2] DRIFT-PATROL.md check table omits the `tracker-server` check

- `scripts/drift-patrol.py:686 check_tracker_server` (str-qwua7.16, 09-07) is
  referenced from AGENTS.md:22 but missing from docs/DRIFT-PATROL.md "What it
  checks".

### D-20 [P2, L3/L6] Terminology decision not reflected anywhere (confirms str-qwua7.57)

- The decision was to unify on "behavior class". Occurrences in
  README+QUICKSTART+SPEC+PROJECT-LAYOUT: "behavior class" 0, "equivalence
  class" 8, "cluster" 10, "path(s)/paths" 26. The CLI prints `**4 path(s)**`
  and `**Behavioral classes:** 4` / `## Class 1`.
- GLOSSARY.md (56 lines) defines strategy/frontier/farming terms and none of
  the nouns users see (behavior map, spec vs snapshot vs report, harness,
  invocation planner, opaque type, stage, provenance).

### D-21 [P3, L3] Tracker IDs and internal process notes in user-facing contracts

- SPEC.md has 10 `str-` references. §2.11 (SPEC.md:643-648) tells users "Do not
  wait for that issue to land before relying on this table as the target
  behavior", which is an internal note inside a contract. README.md:293 is
  headed "Breaking change (str-gg9v)". `--help` output cites 13 distinct
  tracker IDs (e.g. `--observer-pool … See str-frc.3 / str-frc.6`).

### D-22 [P3, L2] Decided removals still documented (confirms str-qwua7.61/.59)

- SPEC.md:699 "Invariants are classified with confidence scores" (removal
  decided in .61). README.md:399 "optional specs or tests" (test emitter
  removal decided in .59, and `test` is documented as "not a test exporter").

### D-23 [P3, L6] `--allow-host-writes` still shown on analysis-only commands

- str-qwua7.15 hid execution-only globals on init/cache/telemetry/spec-diff/doctor
  only. `diff`, `compare`, `stale`, `analyze`, `solve`, `specify`,
  `list-targets`, `nondeterminism`, and `build-frontend` `--help` still list
  `--allow-host-writes` (grep of captured help). SPEC §2.10:591-596 says
  analysis-only commands never execute targets. The help and SPEC should be
  consistent, driven from `command_executes_targets()`.
- In `run --help`/`properties --help`, global options are interleaved with
  command options (e.g. `--output-dir`, `--log-level`, `--max-iterations`,
  `-v`), which makes the help hard to scan.

### D-24 [P3, L3] Crate CLAUDE.md drift (confirms str-qwua7.23/.25; .js refs unfiled)

- Sizes are unchanged since 09-04. shatter-ts/CLAUDE.md:379-380 still names
  `src/browser-globals-recognizer.js` and `src/handlers.js`, but the files are
  `.ts`.

---

## 4. Suggested mechanical checks (cheap, would have caught most findings)

1. **CLI↔SPEC flag inventory** (str-wurp): the per-section script used for
   this review is about 30 lines. It parses `shatter <cmd> --help` for
   `^\s+(-\w, )?--flag`, subtracts the globals, and requires each flag to
   appear in its SPEC §2 section, with an allowlist for "shared with explore".
   Today it reports 2 misses, plus `--failure-threshold` as a reverse miss.
2. **Changelog-row rule**: fail when a diff touches `shatter-cli/src/args.rs`
   or SPEC §2 without touching SPEC §8, unless the commit carries a
   `no-cli-change:` trailer.
3. **Orphan check**: every `docs/**/*.md` must be linked from INDEX.md or from
   a directory README with a Status column.
4. **Artifact round-trip test**: for each documented consumer (`diff`,
   `spec-diff`, `compare`, `stale`, `analyze`, `solve`, `specify`), feed it the
   documented producer's output in an integration test. This catches D-02 and
   D-03.
5. **Sample output from snapshots**: generate SPEC §5 samples from the insta
   snapshot fixtures, not hand-written text.

## 5. Evidence commands (reproducible)

- Help capture: `for c in explore scan …; do shatter $c --help > help/$c.txt; done`
- Flag diff and section diff: `flagdiff.py`, `secdiff.py`, `defdiff.py` (session scratchpad)
- Sandbox repro: see D-01. Diff repro: see D-02.
- Link and orphan check: `links.py` over `git ls-files '*.md'` (114 files)
- Tracker: `bd list --all --limit 0 --json` (1,773 issues), compared with `.beads/issues.jsonl`
