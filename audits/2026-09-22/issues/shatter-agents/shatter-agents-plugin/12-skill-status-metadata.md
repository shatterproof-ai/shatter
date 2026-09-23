---
slug: skill-status-metadata
kind: new
title: "Skill metadata.json status/requires_shatter fields that build-plugins honours, so unreleased skills can live in the catalog without shipping"
priority: P3
type: feature
labels: [packaging, build-plugins, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Skill metadata.json status/requires_shatter fields that build-plugins honours, so unreleased skills can live in the catalog without shipping

## Problem

There is no way to keep a skill in `catalog/plugins.json` while marking it as documenting a future shatter command. `catalog/skills/*/metadata.json` carries only `recommended_model` and `audience`, and `scripts/build-plugins` has no status or requires handling, so every listed skill ships. The two P1 withdrawals from this audit (withdraw-shatter-diff-skill, recipes-marked-design-only) work around this by deleting the skill or dropping it from `plugins.json`. That is enough for now; this mechanism is useful the next time a skill is written ahead of engine support (for example a diff-scoped exploration skill once str-81xiw is under way). It is independent of cli-contract-test, which only reads the built payload and therefore works either way.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `grep -n "status\|requires" scripts/build-plugins` returns 0 matches.
- `scripts/build-plugins` `_is_ignored` (around line 83) skips only `__pycache__` and `.pyc`; `load_catalog` (around line 128) loads every skill named in `plugins.json`.
- Example metadata: `catalog/skills/shatter-advise/metadata.json` has only `recommended_model` and `audience`.

## Acceptance criteria

- [ ] `metadata.json` accepts optional `status` (`released` | `experimental` | `unreleased`, default `released`) and `requires_shatter` (a shatter commit/BUILD tag or an upstream issue id). An unknown `status` value is a build error.
- [ ] `scripts/build-plugins` excludes `unreleased` skills from both the Claude and Codex payloads; `experimental` ships with a banner line prepended to the composed SKILL.md naming `requires_shatter`.
- [ ] `tests/test_build_plugins.py` builds a fixture catalog with one skill of each status and asserts: the unreleased skill is absent from both payloads, the experimental one carries the banner, the released one is unchanged. The test fails before the change.
- [ ] AGENTS.md documents the fields in one short paragraph.
- [ ] `scripts/check-plugins-clean` and `python -m pytest tests/` pass (output pasted in the close comment).

## Out of scope

- Re-shipping shatter-diff or compose-shatter-recipe under the new status. Those are new issues once the engine features exist.

## Dependencies

- None. Related: cli-contract-test, withdraw-shatter-diff-skill, recipes-marked-design-only.

## Priority / Type / Labels

P3 · feature · packaging, build-plugins, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-03 (metadata half). Split from cli-contract-test after the Codex cross-check (finding 4).
