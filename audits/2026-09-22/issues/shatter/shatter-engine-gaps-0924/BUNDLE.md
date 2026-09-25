# Bundle: two engine gaps added 2026-09-24 (repo shatter)

All paths are relative to the shatter repo root (github: shatterproof-ai/shatter), checkout at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22. Both drafts live in bucket shatter-cli-flags-and-help: 07 (extended with parse-failure handling) and 20 (new).

---
slug: unknown-config-keys-warn
kind: new
title: "Config is not validated: a .shatter/config.yaml that fails to parse is accepted, and unknown keys and --set typos are silently ignored"
priority: P2
type: feature
labels: [config, cli, usability, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Config is not validated: unparseable config is accepted; unknown keys and --set typos are silently ignored

## Problem

**Unparseable config.** A `.shatter/config.yaml` that is not valid YAML (for example `foo: [unclosed`) is not reported by any command: `shatter list-targets` exits 0, and `shatter doctor` only checks that the file exists (`shatter-cli/src/commands/doctor.rs:266`, `yaml: ...is_file()`), never parsing it. The user's whole config is then ignored or partly applied with no signal. Found during the 2026-09-22 audit's shatter-agents plugin revision (`issues/shatter-agents/shatter-agents-plugin/REVISION.md`, "Engine-side gaps").

**Unknown keys.** Shatter type-checks config values but not config keys. A typo such as `--set defaults.max_iteratons=5`, or a misspelled key in `.shatter/config.yaml` or `shatter.config.json`, is silently dropped. The run continues with the default value, exits 0, and prints no warning. The user believes the setting took effect. The Go frontend's config loader already warns on unknown top-level keys, so the core is inconsistent with it as well.

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

- [ ] **Parse failures are errors.** Every command that reads `.shatter/config.yaml` or `shatter.config.json` (at least explore, scan, run, list-targets, observe) exits 2 when a config file in the resolution chain fails to parse, naming the file, line and column. `shatter doctor` parses every config file it reports and marks an unparseable one as a failing check (non-zero exit), not just "present". Tests: `foo: [unclosed` in `.shatter/config.yaml` makes `list-targets` exit 2 (it exits 0 on main; record both runs) and makes `doctor` report a failure; a valid config is unaffected.
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
- Parse-failure handling does not depend on the unknown-key work; they may land in either order within this issue.
- Related: str-qwua7.21.1, str-9ee5, help-hides-execution-flags (scopes `--set` to explore).

## Source

Audit 2026-09-22 finding cli-ux-09 (areas/cli-ux.md F9); old draft `drafts/shatter-code/31-unknown-config-keys-warn.md`.


---

---
slug: list-targets-selects-shatter-cache
kind: new
title: "list-targets (TargetManifest) selects Shatter's own generated harness sources under .shatter/cache/ because its default excludes omit .shatter, .git and build"
priority: P2
type: bug
labels: [cli, discovery, scan, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# list-targets selects Shatter's own generated harness sources under .shatter/cache/

## Problem

`shatter list-targets` builds its file list with `TargetManifest::build`
(`shatter-cli/src/commands/list_targets.rs:31` → `shatter-core/src/target_manifest.rs:137`). Its
`DEFAULT_EXCLUDES` (`shatter-core/src/target_manifest.rs:28-37`) covers `node_modules`, `vendor`,
`dist`, `target`, tests and `.d.ts`, but not `.shatter`, `.git` or `build`. After any Rust
exploration, the project contains generated harness sources under `.shatter/cache/harness/` (for
example `.shatter/cache/harness/src/lib.rs`), and `list-targets` selects them as targets.

The native glob walker used for positional wildcard targets already excludes these directories
(`GLOB_WALK_EXCLUDE_DIRS`, `shatter-cli/src/args.rs:2023-2030`: `.git`, `node_modules`, `target`,
`.shatter`, `dist`, `build`). The two discovery paths therefore disagree. Anything that consumes the
manifest, including the shatter plugin's `run-shatter` target discovery, can end up exploring
Shatter's own harness instead of the user's code.

The walker does honour the project's `.gitignore` and `.shatterignore` (`target_manifest.rs:155-156`), so a project that already ignores `.shatter/` hides the bug. It shows in projects and fixtures without that ignore line, for example before `shatter init` or in any repo whose `.gitignore` predates it. Discovery must not depend on the user's ignore file for Shatter's own output.

Found during the 2026-09-22 audit's shatter-agents plugin revision
(`audits/2026-09-22/issues/shatter-agents/shatter-agents-plugin/REVISION.md`, "Engine-side gaps"):
on a project with an existing `.shatter/cache/harness/src/lib.rs`, `list-targets` listed it.

## Acceptance criteria

- [ ] Reproduce on main first: a test fixture directory containing `src/lib.rs` plus
  `.shatter/cache/harness/src/lib.rs`, `.git/hooks/x.rs` and `build/gen.rs`. `list-targets` on it
  selects only `src/lib.rs`. The test fails on main (record the failing assertion), and it lives in
  `shatter-core` (`TargetManifest::build`) so every manifest consumer is covered, not only the CLI.
- [ ] Both discovery paths use one shared exclusion list (one constant, referenced by
  `TargetManifest` and the glob walker), so they cannot drift again. A unit test asserts that the
  glob walker and `TargetManifest` exclude the same directory names.
- [ ] Excluded generated paths are silent, like the existing defaults: they do not appear in the
  manifest's `excluded` list.
- [ ] `shatter scan <dir>` on the same fixture explores no file under `.shatter/`.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Out of scope

- Changing user-configurable include/exclude semantics.
- The shatter plugin's own local pruning in `run_targets.py` (tracked in shatter-agents).

## Related

str-qwua7.56 (closed; lifecycle-export exclusion in target discovery). The shatter-agents drafts
that note this engine gap.


---

