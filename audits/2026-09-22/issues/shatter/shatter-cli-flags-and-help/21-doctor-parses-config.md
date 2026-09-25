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
