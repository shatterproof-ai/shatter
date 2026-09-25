# Documentation Accuracy Review — Shatter

Question: **Do the documentation and the code agree with each other?**

Method: read-only comparison of SPEC.md, README.md, QUICKSTART.md, PLAN.md, all
CLAUDE.md files, AGENTS.md, CONTRIBUTING.md, PROTOCOL.md, PARITY.md, docs/*.md
and demo scripts against `shatter-cli/src/args.rs` (via the pre-captured
`--help` output for all 24 subcommands), `shatter-core/src/{config,spec,
snapshot,checkpoint,scan_orchestrator}.rs`, `shatter-cli/src/{host_writes,
commands/doctor,commands/init}.rs`, `Taskfile.yml`, `install.sh`,
`scripts/docs-smoke.{py,yaml}`, the beads tracker (`.beads/issues.jsonl`), and
`git log --since=2026-07-01` (465 commits). No builds or tests were run.

Short answer: **mostly yes at the flag level, no at the behavior/changelog level.**
The July SPEC overhaul (str-ck05) brought the flag tables into line and the
`docs-smoke` gate keeps shell examples from rotting. But four CLI-visible
behavior changes landed after the overhaul (default-deny host writes, run
`--concolic`, exit-code convention, init/doctor gitignore management) and SPEC's
changelog/"last updated" stamp were not touched for three of them, QUICKSTART's
copy-paste first run no longer works as written, one SPEC config example cannot
deserialize, PARITY.md and docs/CI-INTEGRATION.md describe a project state from
months ago, and docs/INDEX.md omits a third of the docs tree.

---

## P1 — Wrong or breaking

### P1-1 QUICKSTART first-run command is refused by the default-deny host-write policy
- Doc: `QUICKSTART.md:78-88` — "Run Shatter: `./target/release/shatter explore shipping.ts:calculateShipping`" with no mention of a sandbox or opt-in. `grep -i 'sandbox\|allow-host' QUICKSTART.md` → no matches.
- Code: `shatter-cli/src/host_writes.rs:14-15, 98-124` — with no `SHATTER_SANDBOX_BACKEND` and no `--allow-host-writes`/`SHATTER_ALLOW_HOST_WRITES=1`, execution commands (`explore`, `scan`, `run`, `observe`, `properties`, `revalidate`, `bench`) return `Err(refusal_message())` — "refusing to execute target functions without a sandbox". Landed 2026-07-09 (`d787ef18 str-gg9v`), migrated demos/CI 2026-07-10 (`a77fc4be`).
- README.md:290-325 documents the policy correctly (later in the file, under "Executing Target Functions Safely"), but its own "Quick Start"/"Minimal example" at README.md:262-266 also omits the flag.
- Impact: a new user following QUICKSTART §2, §4 and §5 verbatim gets a refusal on every command. `docs-smoke` does not catch this because it only validates flag/subcommand *names*, and its `smoke_commands` are `--help`/`--version` only.

### P1-2 SPEC.md never mentions the default-deny policy or the `--allow-host-writes` global flag
- Doc: `SPEC.md:560-576` §2.10 Global Options table lists 10 globals; `--allow-host-writes` is absent. `grep -n 'allow-host-writes\|ALLOW_HOST_WRITES\|SANDBOX_BACKEND' SPEC.md` → no matches. §2.11 Exit Codes does not say what exit code the refusal produces. No changelog row.
- Code: `help-top.txt` lists `--allow-host-writes` as a global option with a long description citing str-gg9v; `host_writes.rs` implements it.
- SPEC.md:7 says "Any CLI-visible change … should add a row to the changelog" — this is the single largest CLI-visible behavior change since the overhaul and it is entirely missing from the "authoritative reference".

### P1-3 SPEC §3.6 config example uses a form the config parser rejects
- Doc: `SPEC.md:676-687`:
  ```yaml
  functions:
    "src/auth.ts:validateToken":
      max_iterations: 200
      inputs: ["valid-token", "expired-token", "malformed"]
  ```
- Code: `shatter-core/src/config.rs:950-952` — `FunctionConfig.inputs: Option<String>` documented as "Path to a candidate inputs JSON file, relative to the `.shatter/` directory". The in-tree example at `config.rs:1771` is `inputs: ./inputs/validateToken/candidates.json`. A YAML sequence will fail serde deserialization into `Option<String>`.
- `docs-smoke` parses the block with `yaml.safe_load` only — syntactically valid YAML, semantically wrong — so the gate passes.

---

## P2 — Stale / misleading

### P2-1 SPEC changelog and "Last updated" stamp are stale despite three later CLI-visible edits
- `SPEC.md:3` "Last updated: 2026-07-03"; `SPEC.md:1121-1127` changelog last row 2026-07-03.
- `git log -- SPEC.md`: `e4a92afc 2026-07-24 str-yhsp: wire --concolic (and --solver-timeout) through shatter run`; `bf089de0 2026-07-28 str-79t9: document explore's auto-detected invocation planner`; `464e3c9e 2026-08-26 str-9fn2: three-way exit code convention (0/1/2)`. All three edited SPEC body text (§2.4, §2.1, §2.11) without adding a changelog row or bumping the stamp. str-gg9v (P1-2) and str-1fwt/str-mktn (P2-3) never touched SPEC at all.

### P2-2 SPEC §2.11 cites a nonexistent flag `--failure-threshold`
- Doc: `SPEC.md:589` "`scan --fail-on-failures`/`--failure-threshold` tripped".
- Code: `grep -n 'failure.threshold\|failure_threshold' shatter-cli/src/args.rs` → no matches; `help-scan.txt:272` shows only `--fail-on-failures [<PERCENT>]` (the percent form replaced the separate flag). SPEC §2.2 (line 255) documents it correctly — §2.11 is internally inconsistent with §2.2.
- Not caught by `docs-smoke` because it is in prose/table text, not a fenced shell block.

### P2-3 `init`/`doctor` behavior under-documented in SPEC and PROJECT-LAYOUT (str-1fwt, str-mktn)
- `SPEC.md:472-476` §2.8 `init` behavior: creates `.shatter/`, writes `config.yaml`, idempotent. `docs/PROJECT-LAYOUT.md:44-47` "init guarantees creation of: `.shatter/`, `.shatter/config.yaml`".
- Code: `shatter-cli/src/commands/init.rs:16-23` — init "always writes/refreshes the `.gitignore` block" covering every configured generated path (cache, seeds, artifacts, report). `help-doctor.txt:3` — doctor "also checks the target project for any configured output path … that its `.gitignore` fails to cover … Exits non-zero if … an un-ignored generated path is detected", and has a `-d/--directory` flag.
- `SPEC.md:523-527` §2.9 `doctor`: only describes embedded-frontend staleness; no `-d`, no gitignore check, no config-presence/precedence report (`doctor.rs:254-287`, which README.md:222-224 *does* describe).

### P2-4 `shatter.config.json` schema is documented as "full" but omits five fields
- Doc: `README.md:146-190` "`shatter.config.json` — scan-global defaults" table (14 fields); `docs/PROJECT-LAYOUT.md:59-62` "Fields: include, exclude, … capture_side_effects. See the README for the full schema reference."
- Code: `shatter-core/src/config.rs:479-558` `ProjectConfig` also has `parallelism_min`, `parallelism_max`, `observer_pool`, `candidate_queue_capacity`, `coverage_budget_gates` (`CoverageBudgetGates`, lines 567-585, six sub-fields). `help-scan.txt:103,106` confirm these are config-overridable ("Overridden by shatter.config.json when not explicitly set").
- SPEC.md §3.6 never mentions `shatter.config.json` at all except one sentence at `SPEC.md:326`; the two-file config split (str-mktn, 2026-07-13) lives only in README/PROJECT-LAYOUT.

### P2-5 SPEC §5.5 snapshot example does not match `Snapshot` struct
- Doc: `SPEC.md:813-832` — `"version": "0.1.0"`, no `created_at`.
- Code: `shatter-core/src/snapshot.rs:52-58` — `version: u32` (`CURRENT_VERSION = 1`), required `created_at: String`, `SnapshotBehavior` also has `expected_error`. A file shaped like the SPEC example would fail to deserialize (`"0.1.0"` is not a `u32`), so `shatter diff` on it exits 2.
- §5.3 `FunctionSpec` example is consistent with `spec.rs:304-327` (optional fields are `skip_serializing_if`), but note `--spec-out` writes a `FileSpecBundle` (`spec.rs:259-280`: `version`, `file`, `functions[]`, `status`) not a bare `FunctionSpec`; SPEC does not show the bundle shape that `spec-diff`/`stale` actually consume.

### P2-6 SPEC §6.2 checkpoint example partially mismatches `ScanCheckpoint`
- Doc: `SPEC.md:920-930` shows `config_hash` as always present.
- Code: `shatter-core/src/checkpoint.rs:29-45` — `config_hash: Option<String>`. Minor; `version: String` "1" and other fields match.

### P2-7 PARITY.md is stale (last updated 2026-05-13) and cites nonexistent commands
- `PARITY.md:69-70` "Memory limits | N | N | N | Planned (str-9x76)"; "Truncation policy | N | N | N | Planned (str-ebtz)". Tracker: both **closed**. `--memory-limit` is implemented (`args.rs`, `helpers.rs` set `--max-old-space-size`/`GOMEMLIMIT`; SPEC.md:176 documents it).
- `PARITY.md:84` governed commands "`explore`, `scan`, `fuzz`, `reduce`, `re-run`, `run`" — `fuzz`, `reduce`, `re-run` are not subcommands (`help-top.txt`). `helpers.rs:1081` (`cli_parity_tests`) says "explore, scan, and other frontend-spawning subcommands".
- PARITY.md duplicates `protocol/parity-matrix.yaml` (the file CLAUDE.md:15 and DRIFT-PATROL call authoritative) and is not listed in `docs/INDEX.md`.

### P2-8 docs/CI-INTEGRATION.md describes a pre-CI, pre-hooks state
- `docs/CI-INTEGRATION.md:10-12` "The CI platform is intentionally unspecified" — `.github/workflows/ci.yml` exists and runs `task check` (`ci.yml:88-89`); CLAUDE.md:11 documents this.
- `:327-333` "Semgrep CE is not yet configured in-repo" — `.semgrep/shatter.yml` exists. "hook configuration is not yet committed" — `scripts/setup-hooks.sh` and `docs/hooks.md` exist. "generated CLI-doc freshness checks are not yet implemented" — partially true (str-wurp still open) but `task docs-smoke` exists.
- `:268-274` says pre-commit runs `task core:clippy`/`task docs`/`task meta` and pre-push runs `task check`; `docs/hooks.md:19-23` says pre-commit runs `scripts/precommit-rust.sh` and pre-push runs `task affected` (feature) / `task check` (main). hooks.md matches `scripts/setup-hooks.sh:248-249`; CI-INTEGRATION does not.
- `:84-85` "Stage 3: `task e2e`, `task gauntlet-cold`" — fine, tasks exist. `task core:test`, `core:clippy`, `ts:test`, `go:test` exist via Taskfile `includes:` (verified with `task --list-all`).

### P2-9 docs/INDEX.md is incomplete and mischaracterizes PLAN.md
- Not listed anywhere in INDEX: `docs/go-frontend-scope-limits.md`, `docs/performance-profiling.md`, `PARITY.md`, `LANGUAGE-EVALUATION.md`, `ERRORS.md`, and all 34 files under `docs/{audits,ideas,perf,plans,research,specs,superpowers,validation}/` except `validation/kapow-refute-agent-workflow.md`. INDEX has no "archival/plans" section so a reader cannot tell which of these are live.
- `docs/INDEX.md:16` describes PLAN.md as "Roadmap — describes planned/in-progress work"; `PLAN.md:3-8` describes itself as "historical roadmap — not current state … includes features that were … never built". CONTRIBUTING.md:151 repeats the "roadmap" framing.
- `ERRORS.md` is a 9-byte stub (`# Errors`) at repo root.

### P2-10 SPEC "documents every command" but omits subcommand-level detail that exists
- `SPEC.md:490-496` `list-targets`: flags listed ✓. `SPEC.md:498-502` `cache clear --analysis/--results` ✓. `SPEC.md:504-509` workspace gc flags ✓. `SPEC.md:511-513` telemetry actions ✓ (`help-telemetry.txt`).
- Missing: `doctor -d/--directory` (P2-3); `explore`'s `--parallelism-min/--parallelism-max`, `--candidate-queue-capacity` only as parentheticals; scan `--seed`/`--batch` only mentioned inside `--core-sample`'s description.

---

## P3 — Minor / cosmetic

- `shatter-ts/CLAUDE.md:379-380` cites `src/browser-globals-recognizer.js` and `src/handlers.js`; actual files are `.ts` (`ls shatter-ts/src`).
- `CLAUDE.md:41` references `11-opaque-types.ts` / `12-external-deps.ts` — these live in the external examples repo (`/tmp/shatter-examples-main`), matched by `demo/gauntlet-scan-allowlist.yaml:111-112`. Accurate, but not resolvable from this checkout; worth a note.
- `CLAUDE.md` `str-xxxx` references: `str-3op0` (E2E gate Go), `str-o9rz` (E2E gate Rust), `str-emw6` (random explorer ignores `--inputs`) all resolve in `.beads/issues.jsonl` and are meaningful as provenance. Test-tier table (`CLAUDE.md:19-31`) — all nine `task` names exist in `Taskfile.yml`.
- `CONTRIBUTING.md:62-70` project structure omits `shatter-rust-runtime/`, `shatter-llm/`, `shatter-go-tool/`, `shatter-report/`, `shatter-vs/`, `protocol/`, `scripts/`; CLAUDE.md:52 acknowledges four of them.
- `PLAN.md:822-843` CLI design lists `shatter retest`, `shatter spec` — never built; covered by the historical banner but INDEX's framing (P2-9) undercuts that.
- `docs/PROJECT-LAYOUT.md:16-33` Quick Reference paths all resolve to code references (`test_impact.rs`, `harness_storage.rs`, `crypto_registry.rs`, `recorded_mocks.rs`, `helpers.rs` for legacy `.shatter/bin`). ✓
- `docs/DRIFT-PATROL.md:42` references `docs/stories` (missing) and labels it "str-u394l.3 (not implemented)" — accurate; tracker shows open.
- `README.md:47-55` install claims (manifest read, SHA-256 verify, `INSTALL_DIR`, sibling `shatter-rust` binary) match `install.sh:6-22, 93-130, 195-210`. ✓
- `README.md:63-66` prerequisites (Node 22+, Go 1.24+, libclang, Z3) are genuinely required by a plain `cargo build`: `shatter-cli/build.rs:113-117` runs `npm install`/`npm run bundle` at build time. ✓ README/QUICKSTART/CONTRIBUTING agree with each other.
- `docs/distribution.md:80-86` action usage matches `action.yml` inputs (`build`, `channel`, `install-dir`, `shatter-path`, `version`). ✓
- `PROTOCOL.md` command headings (analyze, instrument, prepare, execute, setup, teardown, generate, get_invocation_plan, shutdown) match `protocol/registry.yaml:104-251`; SPEC §4 table matches. ✓
- `demo/gauntlet.sh` + `demo/walkthrough.sh`: every `<subcommand> --flag` pair used (45 unique) exists in the corresponding `--help` (the one "miss", `nondeterminism --cache-dir`, is on the `review` subcommand, whose help was not captured). Both scripts export `SHATTER_ALLOW_HOST_WRITES=1` (`gauntlet.sh:3`), so they run under the new policy. ✓

---

## What `task docs` and `task docs-smoke` verify (Q8)

`task docs` (`Taskfile.yml:352-377`): asserts README/AGENTS/CLAUDE.md/docs exist, then runs `markdownlint-cli2`, `vale`, and `lychee` **only if installed** (`[skip]` otherwise). Lint/prose/link-liveness only — it cannot detect any finding above.

`task docs-smoke` (`Taskfile.yml:334-350`, `scripts/docs-smoke.py`): over README.md, QUICKSTART.md, SPEC.md, docs/INDEX.md it (a) checks every `shatter …` invocation inside ```bash/sh/shell/console fences against the built CLI's clap definitions (unknown subcommand/flag fails), (b) `json.loads`/`yaml.safe_load` on ```json/```yaml fences, (c) runs five `--help`/`--version` smoke commands in a temp dir. It is wired into `check-static` → `task check`.

Would it have caught the findings? **No, none of them:**
- P1-1 (refusal): flag names are valid; the command is never executed.
- P1-2, P2-1, P2-2, P2-3, P2-7, P2-8, P2-9: prose/table text, not fenced code — out of scope.
- P1-3, P2-5, P2-6: YAML/JSON parse fine; no schema validation against `ShatterConfig`/`Snapshot`/`ScanCheckpoint`.
- P2-4: absent fields cannot be detected by parsing an example.
- PARITY.md, CI-INTEGRATION.md, PROJECT-LAYOUT.md, crate CLAUDE.md files are not in `scripts/docs-smoke.yaml` `docs:` at all.

The gate that *would* catch P1-2/P2-2/P2-10 — a mechanical SPEC-vs-clap inventory diff — is `str-wurp` ("Mechanical CLI-surface drift gate"), still **open**; `docs/DRIFT-PATROL.md:41` lists it as "not implemented".

---

## Recommendations (issue-ready)

1. **[P1] QUICKSTART/README first-run examples must work under default-deny.** Add a one-line "Before you run" note to QUICKSTART §2 (and README "Minimal example") showing `export SHATTER_SANDBOX_BACKEND=docker` or `--allow-host-writes`, and add `shatter explore --allow-host-writes …` (or an env-prefixed variant) to `scripts/docs-smoke.yaml` `smoke_commands` on a fixture file so the gate actually executes a target. Files: `QUICKSTART.md:78-88,115-125`, `README.md:262-266`, `scripts/docs-smoke.yaml`.
2. **[P1] SPEC: document str-gg9v.** Add `--allow-host-writes` to §2.10, a "Sandbox and host-write policy" subsection (which commands are gated, env vars, throwaway cwd, TS caveat str-02i70), the refusal's exit code in §2.11, and a changelog row. Files: `SPEC.md:560-596,1121-1127`, source `shatter-cli/src/host_writes.rs`.
3. **[P1] SPEC §3.6: fix `inputs` example** to a path string (`inputs: ./inputs/validateToken/candidates.json`) or, if list-form is desired, change `FunctionConfig.inputs` to an untagged enum. Consider extending `docs-smoke.py` to deserialize ```yaml blocks tagged as config into `ShatterConfig` via a tiny `shatter`-side validator or a JSON schema (there is already a `schemas` task).
4. **[P2] SPEC changelog hygiene.** Add rows for 07-09 str-gg9v, 07-13 str-mktn (two-file config, doctor report), 07-04 str-1fwt (init/doctor gitignore), 07-24 str-yhsp, 07-28 str-79t9, 08-26 str-9fn2; bump "Last updated". Consider a `drift-patrol` check: "SPEC.md changed but changelog table unchanged".
5. **[P2] SPEC §2.11: replace `--failure-threshold` with `--fail-on-failures=PERCENT`.** `SPEC.md:589`.
6. **[P2] SPEC §2.8/§2.9 + PROJECT-LAYOUT: document init's `.gitignore` block, doctor `-d`, doctor's gitignore and config-presence checks.** `SPEC.md:472-476,523-527`, `docs/PROJECT-LAYOUT.md:44-47`.
7. **[P2] `shatter.config.json` schema completeness.** Add `parallelism_min`, `parallelism_max`, `observer_pool`, `candidate_queue_capacity`, `coverage_budget_gates{…}` to README table and PROJECT-LAYOUT field list; add a `shatter.config.json` subsection to SPEC §3.6 (or move the schema there and have README link). Source: `config.rs:479-585`.
8. **[P2] SPEC §5.5/§6.2 examples:** `"version": 1`, add `created_at`, mention `expected_error`; mark `config_hash` optional; add a `FileSpecBundle` example for `--spec-out`. Source: `snapshot.rs:21-58`, `checkpoint.rs:29-45`, `spec.rs:259-280`.
9. **[P2] PARITY.md:** either delete in favor of `protocol/parity-matrix.yaml` (+ move the "CLI Parity Contract" table into SPEC or CLAUDE.md) or refresh: memory limits/truncation → Y, governed-command list → real subcommands, bump date, add to INDEX.
10. **[P2] docs/CI-INTEGRATION.md:** rewrite "Purpose", "Hook Integration Guidance" and "Current Limitations" to reflect `ci.yml`, `.semgrep/`, `scripts/setup-hooks.sh`, `docs/hooks.md`, `task docs-smoke`, `task affected`; or fold into CONTRIBUTING and delete.
11. **[P2] docs/INDEX.md:** add missing top-level docs, add an "Archive: plans/specs/perf/research" section listing `docs/{plans,specs,superpowers,perf,research,ideas,audits,validation}/` with a one-line status, align PLAN.md description with its own banner, delete or populate `ERRORS.md`. Consider a `docs-smoke`/drift-patrol check that every `docs/**/*.md` is referenced from INDEX (the `docs-stories` check in DRIFT-PATROL is the natural home).
12. **[P2] Implement str-wurp** (mechanical clap ↔ SPEC inventory diff). Cheapest form: a script that lists `CliCommand` variants + every long flag from `shatter <cmd> --help` and asserts each appears as a backticked token in SPEC.md §2; add to `check-static`. This would have flagged P1-2, P2-2 and P2-10 automatically.
13. **[P3] Small fixes:** `shatter-ts/CLAUDE.md:379-380` `.js`→`.ts`; CONTRIBUTING project structure list; note in CLAUDE.md:41 that the two fixtures live in the examples repo.
