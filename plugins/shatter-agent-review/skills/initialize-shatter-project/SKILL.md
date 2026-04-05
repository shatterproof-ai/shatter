---
name: initialize-shatter-project
description: Initialize Shatter in a repository by discovering supported-language targets, running `shatter init` for each straightforward target, and adding one native wrapper command named `shatter` to simple command surfaces such as package.json scripts or Taskfile.yml. Use when a downstream user wants project-native Shatter setup rather than ad hoc commands.
---

## Purpose

Use this skill to make Shatter part of a project's existing workflow.

The skill discovers candidate targets first, then applies integration only when
the command surface is simple and unambiguous.

## Default workflow

1. Discover candidate targets:

```bash
python3 ../../scripts/discover_targets.py --root <repo> --json
```

2. Apply initialization and wrapper integration:

```bash
python3 ../../scripts/init_project.py --root <repo> --apply --json
```

3. Review the per-target results.

## Safety rules

- If discovery reports more than four targets, stop and ask before applying.
- Only auto-edit local `package.json` scripts or local `Taskfile.yml` entries in
  v1.
- Prefer the repo's documented task runner when multiple local surfaces exist.
- Do not guess at ambiguous surfaces; use `propose-shatter-integration` to turn
  emitted proposal records into reviewable suggestions.

## Wrapper contract

- Add exactly one wrapper command per integrated target.
- The wrapper name is `shatter`.
- The wrapper should run broad analysis for the target, not a one-off smoke run.
- Default command body: `shatter scan .`

## Notes

- The helper runs `shatter init --directory <target>` before editing wrapper
  files unless `--skip-init` is requested.
- If `shatter` is not on `PATH`, stop and report that explicitly.
