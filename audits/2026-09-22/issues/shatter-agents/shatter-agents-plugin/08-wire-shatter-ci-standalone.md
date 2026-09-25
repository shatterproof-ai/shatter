---
slug: wire-shatter-ci-standalone
kind: new
title: "wire-shatter-ci: template hard-codes plugin-internal run_targets.py against its own fallback guidance, Verify cannot catch it, and install.sh is fetched from main"
priority: P2
type: bug
labels: [skills, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# wire-shatter-ci: template hard-codes plugin-internal run_targets.py against its own fallback guidance, Verify cannot catch it, and install.sh is fetched from main

## Problem

`catalog/skills/wire-shatter-ci/SKILL.md` generates `.github/workflows/shatter.yml` for a downstream repo. Its guidance conflicts with itself, and its Verify step cannot catch the result:

1. **The template and the companion note assume a script the downstream repo usually lacks.** Section 3 ("Determine the run command") correctly says to call `run_targets.py` only if it is vendored in the repo, and otherwise to call each target's native wrapper. But the workflow template in section 4 hard-codes `python3 scripts/run_targets.py --root . --json`, section 3 opens by recommending that same helper, and "Required companion" says the repo may reach it "through the installed Shatter plugin cache", which a GitHub runner does not have. No step vendors the file. An agent that follows the template (the most concrete instruction) produces a workflow that fails on its first run.
2. **Verify does not check that the run command can run.** Section 7 checks the `BUILD=` pin, the upload-artifact step and that "the integrated targets' run command is present", but not that any script path it references exists in the repo.
3. **The installer is not pinned.** The install step pins the binary with `BUILD: continuous-...`, then runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`. Fetching the installer from `main` means a future install.sh change can alter or break a supposedly pinned workflow.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/wire-shatter-ci/SKILL.md:78-91` is "3. Determine the run command": line 83 recommends `python3 scripts/run_targets.py --root . --json`; lines 86-91 say to call it only if vendored, "Otherwise call each target's native wrapper explicitly".
- `SKILL.md:131-137` is the template's install step; line 136 runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`.
- `SKILL.md:139-140` is the template's run step, `python3 scripts/run_targets.py --root . --json`, with no conditional.
- `SKILL.md:175-183` is "7. Verify" (BUILD pin, upload-artifact, run command present).
- `SKILL.md:195-200` is "Required companion" ("vendored in `scripts/` or invoked through the installed Shatter plugin cache").
- Context: shatter currently has no published GitHub release (`gh release list -R shatterproof-ai/shatter` is empty; tracked in the shatter repo as <release-publish-and-install-smoke id>, audit slug `release-publish-and-install-smoke`), so no `BUILD=` value resolves today. That blocks an end-to-end run, not this fix.

## Acceptance criteria

- [ ] The template's run step no longer hard-codes `scripts/run_targets.py`. It is a placeholder filled from section 3's decision: the project's native wrappers (`npm run shatter`, `task shatter`, `make shatter`) or `shatter scan`, or `run_targets.py` only when the skill has confirmed the file exists in the repo. "Required companion" no longer mentions the plugin cache as a CI option.
- [ ] Verify (section 7) additionally checks that every repo-relative script path in a `run:` step exists in the target repo, and that the install URL uses the pinned ref.
- [ ] `install.sh` is fetched from the pinned tag or commit (for example `https://raw.githubusercontent.com/shatterproof-ai/shatter/<pinned-ref>/install.sh`), derived from the same value as `BUILD`, not from `main`.
- [ ] A test in `tests/` renders the workflow as the skill instructs for two fixture repos (one with wrappers and no vendored helper, one with a vendored `scripts/run_targets.py`), parses the YAML, and asserts that every script path in a `run:` step exists in the fixture and that the install URL contains the pinned ref. The rendering logic lives in a helper under `catalog/skills/wire-shatter-ci/scripts/` so the test exercises what the skill uses. The test fails against the current template (the no-helper fixture gets `scripts/run_targets.py`).
- [ ] Optional, once shatter publishes releases: a transcript of the generated workflow running green under `act` or in a scratch GitHub repo, linked in the close comment.

## Suggested approach

Remove the dependency on `run_targets.py` from the CI path. The workflow already knows the project's wrappers, or can call `shatter scan . -o shatter-review/report.json` with the execution opt-in chosen in sa-oio. Derive the install URL from the same pinned value as `BUILD`.

## Out of scope

- Publishing shatter releases. That is shatter <release-publish-and-install-smoke id>.
- The execution-policy decision for wrappers. That is sa-oio.

## Dependencies

- None within this tracker.
- Related: sa-oio (wrapper body and execution opt-in), cli-contract-test (flags in the template).
- Cross-repo: shatter <release-publish-and-install-smoke id>, only for the optional live-run proof.

## Priority / Type / Labels

P2 · bug · skills, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-11. Revised after the Codex cross-check (findings 8, 10).
