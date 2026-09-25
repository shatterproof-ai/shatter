---
slug: delegate-discovery-to-engine
kind: new
title: "shatter-doctor should report the engine's own `shatter doctor` and move its PyYAML-dependent config check into a tested helper"
priority: P2
type: task
labels: [skills, shatter-doctor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# shatter-doctor should report the engine's own `shatter doctor` and move its PyYAML-dependent config check into a tested helper

## Problem

The `shatter-doctor` skill diagnoses a project's Shatter setup without ever calling the engine's own diagnostic, `shatter doctor`. That command already reports the install (version, embedded frontend hashes, stale Go embed), which project config files are present, and whether generated output paths are git-ignored. The skill re-derives part of this and misses the rest.

Its one engine-independent check, parsing `.shatter/config.yaml`, is an inline `python3 -c "import sys, yaml; ..."` in SKILL.md prose. It needs PyYAML (not in the standard library), silently degrades to a byte count when PyYAML is missing, and cannot be tested because it is not code in the repo.

(The draft originally also proposed replacing `run_targets.py`'s tree walk with `shatter list-targets`. The cross-check showed that is not viable: `list-targets` returns source files, not package roots, and itself selects generated harness sources. That part became the note on sa-d8j, sa-d8j-engine-discovery-note.)

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and a shatter CLI built from shatter `70465921`.

- `catalog/skills/shatter-doctor/SKILL.md:71` has `python3 -c "import sys, yaml; yaml.safe_load(open(sys.argv[1]))" .shatter/config.yaml`. Lines 78-80 are the fallback "parse skipped: pyyaml not installed, file is N bytes". `catalog/skills/shatter-doctor/` has no `scripts/` directory.
- `grep -n "shatter doctor"` over `catalog/skills/shatter-doctor/SKILL.md` returns 0 matches.
- `shatter doctor --help` (args.rs:1777): `-d, --directory <DIRECTORY>`; it prints version and frontend hashes, a "Project configuration" block (presence of `shatter.config.json` and `.shatter/config.yaml`, precedence), embed staleness, and un-ignored generated paths, and exits 1 when an embed is stale or a generated path is not ignored. It does **not** parse or validate `.shatter/config.yaml`.
- The engine does not reject a malformed config either: with `.shatter/config.yaml` containing `foo: [unclosed`, `shatter list-targets --format json .` exits 0. So the skill's parse check is currently the only config validation a user gets.

## Acceptance criteria

- [ ] shatter-doctor runs `shatter doctor -d <root>` (flags as shown by the current build's `--help`) and includes its output, and its exit status, in the report. A nonzero `shatter doctor` exit is surfaced as a finding, not treated as a skill failure. The report gains the engine's embed-staleness and un-ignored-generated-path findings, which the skill does not check today; the skill's own version and config-presence steps (SKILL.md sections 1-2) may be simplified to reuse `shatter doctor` output. If the binary is missing, the existing "run install-shatter" path is unchanged.
- [ ] The config parse check moves into `catalog/skills/shatter-doctor/scripts/check_config.py` and SKILL.md calls it. It prints exactly one of: `config: missing`, `config: parsed OK`, `config: parse error at line N: <message>`, or `config: unverified (PyYAML unavailable)`. The last case is reported as unverified in the skill's summary, never as OK.
- [ ] `tests/test_shatter_doctor_config.py` runs the helper as a subprocess under `python3 -I -S` (so PyYAML is not importable) on a fixture and asserts the `unverified` line and exit 0; and, skipped with a reason when PyYAML is absent from the test environment, asserts `parsed OK` for a valid fixture and `parse error at line 1` for `foo: [unclosed`. The test fails before the change (the file does not exist).
- [ ] The helper is copied into both payloads (`find plugins -path '*shatter-doctor/scripts/check_config.py'` prints two paths).
- [ ] `python -m pytest tests/` and `scripts/check-plugins-clean` pass (output pasted in the close comment), plus a transcript of the skill's report on one real project showing the `shatter doctor` section.

## Suggested approach

Keep the helper stdlib-only apart from the optional `import yaml` inside a try block. The stale shatter-diff hook check from withdraw-shatter-diff-skill can live alongside it in the same `scripts/` directory.

## Out of scope

- Target discovery in `run_targets.py` (see sa-d8j and the sa-d8j-engine-discovery-note).
- Config validation in the engine (`shatter doctor` parsing `.shatter/config.yaml`, or commands rejecting a malformed config). That is a shatter-repo request; see the bundle's cross-repo notes.

## Dependencies

- None within this tracker. Related: withdraw-shatter-diff-skill (adds a hook check to the same skill; either can land first), cli-contract-test.

## Priority / Type / Labels

P2 · task · skills, shatter-doctor, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-10. Revised after the Codex cross-check (findings 1, 2): the run_targets.py delegation was dropped as unworkable and moved to a note on sa-d8j. Slug kept for stability although the title narrowed.
