---
slug: wire-shatter-ci-standalone
kind: new
title: "wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main"
priority: P2
type: bug
labels: [skills, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main

## Problem

`catalog/skills/wire-shatter-ci/SKILL.md` generates `.github/workflows/shatter.yml` for a downstream repo. The generated workflow has two defects:

1. **It depends on a script the downstream repo does not have.** The run step is `python3 scripts/run_targets.py --root . --json`. `run_targets.py` lives inside the plugin (`catalog/skills/run-shatter/scripts/`). The skill's "Required companion" section says the repo "must have access to it (either vendored in scripts/ or invoked through the installed Shatter plugin cache)". A GitHub runner has no plugin cache, no skill step vendors the file, and the skill's Verify step checks only the `BUILD=` pin and the upload-artifact step. The generated workflow therefore fails on its first run.
2. **The installer is not pinned.** The install step sets `BUILD: continuous-...` to pin the binary, but then runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`. That fetches the installer script from `main`, so a future install.sh change can alter or break a supposedly pinned workflow.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/wire-shatter-ci/SKILL.md:65-72` is "Choose a pinned `BUILD=` tag" (`BUILD=continuous-YYYYMMDD-HHMM-<sha>`).
- `SKILL.md:131-137` is the install step, which runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`.
- `SKILL.md:139-140` is the run step, `python3 scripts/run_targets.py --root . --json`.
- `SKILL.md:175` is "7. Verify" (it checks BUILD and upload-artifact only).
- `SKILL.md:195-200` is "Required companion" ("vendored in scripts/ or invoked through the installed Shatter plugin cache").
- Context: shatter currently has no published GitHub release (`gh release list -R shatterproof-ai/shatter` is empty; tracked in the shatter repo as `release-publish-and-install-smoke`), so no `BUILD=` value resolves today. That is a shatter-side blocker for running the workflow end to end, not a defect in this skill.

## Acceptance criteria

- [ ] The generated workflow does not reference any file the skill has not written into the target repo. Preferred: it runs the project's own wrappers (`npm run shatter`, `task shatter`, `make shatter`) or native `shatter scan` directly. Alternatively, the skill vendors `run_targets.py` with a version header and a documented update path, as an explicit step that its Verify section checks.
- [ ] `install.sh` is fetched from the pinned tag or commit (for example `https://raw.githubusercontent.com/shatterproof-ai/shatter/<pinned-ref>/install.sh`), not from `main`.
- [ ] A test in `tests/` renders the workflow template for a fixture repo, parses the YAML, and asserts that every script path referenced in a `run:` step exists in the fixture after the skill's steps, and that the install URL contains the pinned ref. The test fails against the current template.
- [ ] Optional, once shatter publishes releases: a transcript of the generated workflow running green under `act` or in a scratch GitHub repo, linked in the close comment.

## Suggested approach

Remove the dependency on `run_targets.py` from the CI path. The workflow already knows the project's wrappers, or can call `shatter scan . -o shatter-review/report.json` with the execution opt-in chosen in sa-oio. Derive the install URL from the same pinned value as `BUILD`.

## Out of scope

- Publishing shatter releases. That is shatter `release-publish-and-install-smoke`.
- The execution-policy decision for wrappers. That is sa-oio.

## Dependencies

- None within this tracker.
- Related: sa-oio (wrapper body and execution opt-in), cli-contract-test (flags in the template).
- Cross-repo: shatter `release-publish-and-install-smoke`, only for the optional live-run proof.

## Priority / Type / Labels

P2 · bug · skills, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-11.
