---
name: run-shatter
description: Run an installed Shatter binary against a file, function, or project and capture reproducible review artifacts for downstream analysis. Use when a downstream user wants to explore behavior, scan a project, or generate a spec before reviewing results.
---

## Purpose

This skill is for downstream users of Shatter, not maintainers of the Shatter repo.

Use it to choose the right Shatter command, run it, and save enough context for later review.

## Defaults

- Prefer `shatter` on `PATH`.
- If `shatter` is not available, stop and report the missing binary instead of guessing.
- Create a dedicated run directory such as `shatter-review/<timestamp>/`.
- Save the exact command, Shatter version, captured console output, and generated artifact paths.

## Command selection

- Use `shatter explore <file>:<function>` for one function.
- Use `shatter explore <file>` when the user wants all exported functions in one file.
- Use `shatter scan <path>` for a directory or multi-file project pass.
- Add `--spec-json --spec-out <path>` when later review needs precise machine-readable evidence.
- Keep extra flags explicit in the saved command so the run is reproducible.

## Capture requirements

For every run, preserve:

- the exact command line
- the working directory
- `shatter --version`
- stdout and stderr
- any spec, report, or export files written by the command

If the command produces both human-readable output and a JSON spec, keep both. The review skill can use the human-readable output for explanation and the JSON spec for precise evidence.

## Handoff

End with a short run summary that names:

- the target that was explored
- whether the run completed or failed
- the files saved in the run directory

Pass those artifact paths to `review-shatter-output` and `report-shatter-issues`.
