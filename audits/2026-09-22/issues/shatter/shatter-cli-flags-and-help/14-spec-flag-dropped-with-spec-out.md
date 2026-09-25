---
slug: spec-flag-dropped-with-spec-out
kind: new
title: "explore --spec is silently ignored when --spec-out is also given"
priority: P3
type: bug
labels: [cli, explore, spec, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore --spec is silently ignored when --spec-out is also given

## Problem

`explore ... --spec --spec-out spec.json` writes the spec file but prints no spec to stdout and no warning. The user asked for both.

## Evidence

- `shatter-cli/src/main.rs:418-424` passes `spec_out` and folds `--spec`, `--spec-json` and `--spec-out` into combined booleans (`spec || spec_json || spec_out.is_some() || invariants`, `spec_json || spec_out.is_some()`), so the stdout request is not distinguished once `--spec-out` is set.
- Finding artifacts-16 (P3); not independently reproduced in this revision.

## Acceptance criteria

- [ ] Decide and document one behavior: `--spec` with `--spec-out` prints the spec to stdout as well as writing the file, or clap rejects the combination with a usage error (exit 2). Silent dropping is not allowed.
- [ ] A CLI test covers the combination and fails on current HEAD (stdout lacks the spec and stderr has no message).
- [ ] The `--spec` / `--spec-out` help text states the chosen behavior.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.
- Related: explore-report-printed-twice (stdout emission rules).

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 6.
