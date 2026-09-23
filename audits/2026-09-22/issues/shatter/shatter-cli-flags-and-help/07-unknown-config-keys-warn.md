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
