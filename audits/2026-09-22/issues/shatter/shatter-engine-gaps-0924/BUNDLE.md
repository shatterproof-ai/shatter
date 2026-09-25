# Bundle: engine gaps added 2026-09-24 (repo shatter), revision 2

All paths are relative to the shatter repo root (github: shatterproof-ai/shatter), checkout at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22. All three drafts live in bucket shatter-cli-flags-and-help. 07 (unknown-config-keys-warn) is included for context only: it is restored, unchanged, to its content before commit 3c394596, because the parse-failure extension was wrong (explore already exits 2 on a config YAML parse error, and list-targets never reads .shatter/config.yaml). 20 (list-targets-selects-shatter-cache) is narrowed to excluding Shatter-managed .shatter/ output from TargetManifest selection. 21 (doctor-parses-config) is new and carries the one real parse-failure gap: doctor reports an unparseable config as present.

---
slug: unknown-config-keys-warn
kind: new
title: "Warn on unknown config keys and --set key typos (currently silently ignored)"
priority: P2
type: feature
labels: [config, cli, usability, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Warn on unknown config keys and --set key typos (currently silently ignored)

## Problem

Shatter type-checks config values but not config keys. A typo such as `--set defaults.max_iteratons=5`, or a misspelled key in `.shatter/config.yaml` or `shatter.config.json`, is silently dropped. The run continues with the default value, exits 0, and prints no warning. The user believes the setting took effect. The Go frontend's config loader already warns on unknown top-level keys, so the core is inconsistent with it as well.

A related silent drop: `--set` is a global flag, but only `explore` applies it (`main.rs:437` is the only reader of `cli.set_overrides`). `scan --set ...` and `run --set ...` accept the flag and ignore it entirely, so even a correctly spelled key has no effect there. help-hides-execution-flags removes `--set` from those commands; until it lands, this issue makes the drop visible.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`):

- `shatter-core/src/config.rs:4178-4185`, test `parse_set_overrides_unknown_field_is_ignored_by_serde`, with the comment "ShatterConfig derives Deserialize without it [deny_unknown_fields], so unknown keys are silently ignored." The test asserts only "no panic". It pins the lenient behavior.
- `shatter-core/src/config.rs:1104` `parse_set_overrides`.
- Config load sites that consume `--set`:
  - the main per-function path: `shatter-core/src/config.rs:1597` `resolve_function_config_with_inputs(..., set_overrides)`, called from `shatter-cli/src/commands/explore.rs:4832-4838`;
  - the LLM path: `shatter-cli/src/helpers.rs:1571` `resolve_llm_config` (`:1584-1585` calls `parse_set_overrides`), called from `explore.rs:4314`.
- `--set` is read only in the explore dispatch (`shatter-cli/src/main.rs:437`); no scan or run code reads `set_overrides`.
- Go frontend contrast: `shatter-go/config/loader.go:431-444` emits `config <path>: ignoring unknown top-level key "<k>"`.
- Observed (findings.json cli-ux-09, reproduced by the verifier): `explore ... --set defaults.max_iteratons=5` exits 0 with nothing about the key on stderr (`audits/2026-09-22/cli-ux-transcripts/err-setunknown.err`). `--set foo.bar=1` exits 0. `--set defaults.max_iterations=abc` exits 2 (`err-badset.err`).
- `strsim` is already in `Cargo.lock`.

## Acceptance criteria

- [ ] An unknown key in `.shatter/config.yaml`, in `shatter.config.json`, or in a `--set KEY=VALUE` override produces exactly one warning on stderr per key per run, at every load site listed above (per-function config and LLM config). The warning names the source (file path or `--set`), the full dotted key path and, when a known key is close, a "did you mean `defaults.max_iterations`?" suggestion.
- [ ] A strict mode turns those warnings into a usage error (exit 2). Pick one form (`--strict-config` flag, config key, or env var), document it in the config reference, and test it.
- [ ] `parse_set_overrides_unknown_field_is_ignored_by_serde` is replaced by tests that assert (a) the warning and suggestion for a typo in `--set` on `explore`, (b) the same for a typo in a YAML config file, (c) exit 2 in strict mode, and (d) no warning for a valid config (use the repo's own example configs and `demo/` configs as a no-false-positive check). Tests (a) and (b) fail before the change; the close comment records both runs.
- [ ] `--set` on a command that does not apply it is not silent: either it is rejected by clap (if help-hides-execution-flags has landed) or the command prints `warning: --set is ignored by <cmd>` on stderr. A CLI test covers `scan <dir> --set defaults.max_iterations=5` and asserts whichever of the two applies at close time.
- [ ] Keys that are legitimately open-ended (maps keyed by user data, if any) do not warn. List them in the close comment.
- [ ] Warnings go to stderr only, so JSON stdout contracts (`shatter-cli/tests/json_stdout_contract.rs`) still pass.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Wrap the deserializer at each config load site, and in `parse_set_overrides`, with `serde_ignored`, collecting the ignored paths. Match them against the known key set with `strsim` to produce suggestions. Deduplicate across load sites (the same `--set` pairs are parsed per function and for the LLM config). Avoid `deny_unknown_fields` as the default because it rules out warn-only mode and forward compatibility.

## Out of scope

- A generated config reference document (str-qwua7.21.1).
- Validating free-string format values for stale/revalidate (str-9ee5).
- Changes to the Go frontend loader, which already warns.
- Making scan/run apply `--set` (see help-hides-execution-flags).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.21.1, str-9ee5, help-hides-execution-flags (scopes `--set` to explore).

## Source

Audit 2026-09-22 finding cli-ux-09 (areas/cli-ux.md F9); old draft `drafts/shatter-code/31-unknown-config-keys-warn.md`.

---

---
slug: list-targets-selects-shatter-cache
kind: new
title: "list-targets (TargetManifest) selects Shatter's own generated harness sources under .shatter/ when the project's .gitignore does not exclude it"
priority: P2
type: bug
labels: [cli, discovery, scan, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# list-targets selects Shatter's own generated harness sources under .shatter/

## Problem

Shatter-managed output under `.shatter/` must never be selected as a scan target, whatever the
user's `.gitignore` says. Today it is selected unless the user's ignore file happens to exclude it.

`shatter list-targets` builds its file list with `TargetManifest::build`
(`shatter-cli/src/commands/list_targets.rs:31` → `shatter-core/src/target_manifest.rs:137`). Its
`DEFAULT_EXCLUDES` (`shatter-core/src/target_manifest.rs:28-37`) covers `node_modules`, `vendor`,
`dist`, `target`, `__tests__`, test files and `.d.ts`, but not `.shatter`. After any Rust
exploration, the project contains generated harness sources under `.shatter/cache/harness/` (for
example `.shatter/cache/harness/src/lib.rs`), and `list-targets` selects them.

The walker honours the project's `.gitignore` and `.shatterignore`
(`target_manifest.rs:155-156`), so a project whose `.gitignore` already lists `.shatter/` hides the
bug. It shows in projects without that line, for example before `shatter init` or in any repo whose
`.gitignore` predates it. Anything that consumes the manifest, including the shatter plugin's
`run-shatter` target discovery, can then explore Shatter's own harness instead of the user's code.

Precedent: the native glob walker used for positional wildcard targets already skips `.shatter`
(`GLOB_WALK_EXCLUDE_DIRS`, `shatter-cli/src/args.rs:2023-2030`). The two paths use different
matching (directory names in `shatter-cli` versus glob patterns in `shatter-core`), so this issue
does not require them to share one constant.

Found during the 2026-09-22 audit's shatter-agents plugin revision
(`audits/2026-09-22/issues/shatter-agents/shatter-agents-plugin/REVISION.md`, "Engine-side gaps").

## Evidence

Reproduced 2026-09-24 with the audit checkout's debug binary
(`target/debug/shatter`, built 2026-09-22) on a fresh fixture with no `.gitignore`, no
`.shatterignore` and no `.git`, containing only `src/lib.rs` and
`.shatter/cache/harness/src/lib.rs`:

```
$ shatter list-targets .
Target manifest — <fixture>
  config hash:      170de33e...
  source set hash:  07aae94e...

Selected (2):
  .shatter/cache/harness/src/lib.rs  [rust, 1 lines]
  src/lib.rs  [rust, 1 lines]
```

Exit status 0.

## Acceptance criteria

- [ ] Reproduce on main first with a `shatter-core` test on `TargetManifest::build`, so every
  manifest consumer is covered, not only the CLI. Fixture: `src/lib.rs` plus
  `.shatter/cache/harness/src/lib.rs`, with no `.gitignore` or `.shatterignore`. Assert that only
  `src/lib.rs` is selected. The test fails on main; the close comment records the failing assertion.
- [ ] `TargetManifest` excludes everything under `.shatter/` by default, regardless of the
  project's `.gitignore`/`.shatterignore`.
- [ ] The exclusion is silent, like the existing defaults: `.shatter/` paths do not appear in the
  manifest's `excluded` list.
- [ ] `shatter list-targets` on the fixture above lists only `src/lib.rs`.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Optional consideration

The glob walker also skips `.git` and `build`, which `DEFAULT_EXCLUDES` does not. Whether the
manifest should skip them too is a separate decision (`build/` can hold user sources in some
projects). Not required here; if the implementer adds them, note it in the close comment and add
test cases for each.

## Out of scope

- Changing user-configurable include/exclude semantics.
- Unifying the glob walker's and the manifest's exclusion lists.
- `shatter scan`'s own file discovery. In the audit checkout, the only non-test caller of
  `TargetManifest::build` is `list_targets.rs` (`:31`, `:222`); scan does not go through it.
- The shatter plugin's own local pruning in `run_targets.py` (tracked in shatter-agents).

## Related

str-qwua7.56 (closed; lifecycle-export exclusion in target discovery). The shatter-agents drafts
that note this engine gap.

---

---
slug: doctor-parses-config
kind: new
title: "shatter doctor reports .shatter/config.yaml as \"present\" even when it fails to parse"
priority: P3
type: bug
labels: [cli, config, doctor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter doctor reports an unparseable config as "present"

## Problem

`shatter doctor` is the command a user runs to check a Shatter setup, but its "Project
configuration" section only checks that each config file exists. It never parses them.
`detect_config_presence` (`shatter-cli/src/commands/doctor.rs:261-268`) sets
`yaml: root.join(".shatter").join("config.yaml").is_file()` (`:266`) and
`project_json: ...PROJECT_CONFIG_FILENAME).is_file()` (`:263-265`), and `config_report_lines`
(`:274`) prints `present` or `not found`. A `.shatter/config.yaml` with a YAML syntax error is
therefore reported as `present` while `explore` refuses to run with it. `doctor` exits 0 if its
other checks pass.

The parsers already exist and report errors: `shatter_core::config::parse_config`
(`shatter-core/src/config.rs:1164`, returns `ConfigError::Parse` for invalid YAML) and
`find_project_config` (`config.rs:642`, returns `ConfigError::ProjectConfigParse` for invalid JSON).

Found while re-checking the 2026-09-22 audit's engine-gap drafts (Codex review
`audits/2026-09-22/issues/crosscheck/shatter-engine-gaps-0924.codex.md`). The earlier draft claimed
other commands also accept an unparseable config; that was wrong (see Evidence), and this issue is
limited to `doctor`.

## Evidence

Reproduced 2026-09-24 with the audit checkout's debug binary (`target/debug/shatter`, built
2026-09-22). Fixture: `src/lib.rs`, `.shatter/config.yaml` containing `foo: [unclosed`, and a
`.gitignore` covering the generated paths so doctor's gitignore check passes.

```
$ shatter doctor --directory <fixture>
...
Project configuration
  shatter.config.json:   not found  scan-global: ...
  .shatter/config.yaml:  present    per-function: ...
...
Generated-path gitignore: all configured output paths are ignored.
```

Exit status 0.

```
$ shatter explore src/lib.rs:f --allow-host-writes
Error: failed to parse config YAML '.shatter/config.yaml': did not find expected ',' or ']' at line 2 column 1, while parsing a flow sequence at line 1 column 6
```

Exit status 2.

## Acceptance criteria

- [ ] `doctor` parses each config file it reports, reusing `shatter_core::config::parse_config`
  for `.shatter/config.yaml` and `find_project_config` (or the same `serde_json` parse it uses)
  for `shatter.config.json`. It does not add a second parser.
- [ ] A file that fails to parse is shown as `invalid: <error>` in place of `present`. The error
  text includes the line and column from the parser.
- [ ] An invalid config makes `doctor` exit non-zero, like its existing failing checks
  (`run_doctor` returns `Ok(false)`).
- [ ] Tests with a fixture: (a) `foo: [unclosed` in `.shatter/config.yaml` gives `invalid:` with
  line/column and a failing result; (b) invalid JSON in `shatter.config.json` does the same;
  (c) valid configs are still reported as `present` and do not fail. Test (a) fails on main; the
  close comment records both runs.
- [ ] No behaviour change in any other command.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Out of scope

- Unknown-key warnings and `--set` typo handling (unknown-config-keys-warn).
- Semantic validation of config values beyond what the existing parsers do.

## Related

unknown-config-keys-warn (same audit bucket). str-mktn (added the config-presence report).

---

