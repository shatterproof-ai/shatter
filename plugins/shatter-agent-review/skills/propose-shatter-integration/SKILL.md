---
name: propose-shatter-integration
description: Turn ambiguous `init_project.py` results into reviewable Shatter integration suggestions, including concrete patch shapes and wrapper commands for local and ancestor command surfaces. Use when initialization discovers multiple viable surfaces or only ancestor-owned surfaces and a human should review the exact proposed edits before applying anything.
---

## Purpose

Use this skill after target discovery or dry-run initialization reports
ambiguous targets.

It does not apply edits. It converts proposal records into a short review set a
human can approve, reject, or refine.

## Default workflow

1. Generate proposal records:

```bash
python3 ../../scripts/init_project.py --root <repo> --skip-init --json
```

2. For each target with `status: "ambiguous"` and non-empty `proposals`, present:
   - target root and detected languages
   - why the target was ambiguous
   - one item per proposal with the candidate surface path, scope, and exact
     wrapper command
   - the concrete edit shape from `proposal.edit`

## How to present each proposal

- For `package-json-script`, show a patch suggestion that adds
  `scripts.shatter = "<script_value>"`.
- For `taskfile-task`, show a patch suggestion that adds a `shatter` task with
  the recorded `desc` and `cmds`.
- For ancestor surfaces, explicitly call out that the wrapper command was
  rewritten to target the child path rather than `.`.

## Decision rules

- Prefer repo documentation hints when they make one surface clearly canonical.
- If multiple local surfaces remain plausible, present all of them; do not
  collapse them into a guess.
- If only ancestor surfaces are available, call out the ownership/scoping tradeoff.
- If the repo already defines a conflicting `shatter` entry, stop and flag the
  conflict instead of proposing an overwrite.
