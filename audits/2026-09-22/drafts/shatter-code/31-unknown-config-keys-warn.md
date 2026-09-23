# Unknown config keys and `--set` key typos are silently ignored

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | feature |
| priority | P2 |
| labels | config,cli,usability,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | cli-ux-09 |

<!-- body -->
## Problem

`--set defaults.max_iteratons=5` and `--set foo.bar=1` exit 0 with no warning; values are type-checked but keys are not. Unknown keys in `.shatter/config.yaml` / `shatter.config.json` are likewise dropped (the Go frontend warns, the core does not).

## Current code facts / evidence

- `shatter-core/src/config.rs:4180-4182` test comment: 'ShatterConfig derives Deserialize without it [deny_unknown_fields], so unknown keys are silently ignored.'
- `shatter-cli/src/helpers.rs:1574-1590` --set handling.
- `--set defaults.max_iterations=abc` → exit 2 (values checked).

## Acceptance criteria

- Unknown keys in config files and `--set` produce a warning with a 'did you mean' suggestion (e.g. serde_ignored + strsim).
- `--strict-config` (or config key) turns them into errors.
- The test that pins lenient behaviour is updated; new tests for typo warning and strict mode.

## Suggested approach

Use serde_ignored at deserialize sites.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: cli-ux-09 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
