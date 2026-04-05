---
name: discover-shatter-targets
description: Discover Shatter integration targets in a repository by inspecting project documentation, supported-language build roots, and likely command surfaces such as Taskfile, package.json scripts, justfile, and Makefile. Use before adding or running project-native Shatter wrapper commands.
---

## Purpose

Use this skill before integrating or running Shatter across a repository.

It identifies supported-language project roots, likely command surfaces, and
whether the repo is large enough to require confirmation before broad changes.

## Default workflow

Run the detector first:

```bash
python3 ../../scripts/discover_targets.py --root <repo> --json
```

If you need a quick human summary instead of JSON:

```bash
python3 ../../scripts/discover_targets.py --root <repo>
```

## What the detector should decide

- Supported-language target roots (`Cargo.toml`, `go.mod`, `package.json`)
- Likely command surfaces for each target
- Whether a surface is local to the target or inherited from an ancestor
- Whether common generated or vendored directories were excluded
- Whether the repo exceeds the four-target confirmation threshold

## Interpretation rules

- Prefer repo documentation hints when they clearly indicate a canonical task
  runner.
- Treat `Taskfile.yml`, `package.json` scripts, `justfile`, and `Makefile` as
  candidate command surfaces.
- Preserve nested targets in the output; later skills can decide whether to
  integrate the root, the child, or both.
- Do not guess wrapper commands yet. This skill only discovers targets and
  likely command surfaces.
