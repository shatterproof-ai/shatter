---
slug: agents-md-structure-and-readme
kind: new
title: "AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules and their sync rules, and a README"
priority: P3
type: chore
labels: [audit-2026-09-22, docs]
parent_epic: "Epic: Audit 2026-09-22 findings (bugshot)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules and their sync rules, and a README

## Problem

Bugshot ships four skills: `bugshot`, `vizline`, `vizdiff` and `wire-bugshot`.
The repo's `AGENTS.md` was written when only `bugshot` existed, and the
additions since then were never documented:

- **Project Structure** omits the viz/wire entry points and workflows, and the
  shared modules they use.
- **Documentation Sync Rules** cover only `skills/bugshot/SKILL.md`. Nothing
  tells an agent that changing `capture_runner.py`, `image_diff.py`,
  `baseline_manifest.py` or the `wire_bugshot_*` flags requires updating the
  viz/wire `SKILL.md` files, or rebuilding the per-skill copies of those modules.
- The repo has no `README.md`, so a human visiting the GitHub repo sees no
  description, install steps or skill list. INSTALL.md exists.

An agent editing viz code gets no signal that docs must move with it. Doc
drift of exactly this kind (ANSI support that exists but is not documented)
is what bgs-3tq is fixing on the CLI capture side.

## Evidence

Re-verified 2026-09-23 against `/home/ketan/project/bugshot` HEAD `e622d73`:

- `AGENTS.md:37-50` "Project Structure" lists `bugshot_cli.py`,
  `bugshot_workflow.py`, `gallery_server.py`, `ansi_render.py`,
  `static/`/`templates/`, `skills/bugshot/SKILL.md`, `skills/bugshot/overlays/`,
  generated `.claude/skills/`/`.codex-plugin/skills/`, one spec doc and `tests/`.
- It does **not** list any of these, which exist at the repo root:
  `vizline_cli.py`, `vizline_workflow.py`, `vizdiff_cli.py`,
  `vizdiff_workflow.py`, `vizdiff_review_root.py`, `wire_bugshot_cli.py`,
  `wire_bugshot_workflow.py`, `capture_runner.py`, `image_diff.py`,
  `baseline_manifest.py`, or the skill dirs `skills/vizline/`,
  `skills/vizdiff/` and `skills/wire-bugshot/`.
- Shared modules are copied into skill dirs, and the copies are git-tracked:
  - `skills/vizline/` holds `capture_runner.py`, `image_diff.py` and
    `baseline_manifest.py`.
  - `skills/wire-bugshot/` holds `image_diff.py` and `baseline_manifest.py`.
  - `skills/vizdiff/` holds those three modules. It also holds copies of
    bugshot core modules: `ansi_render.py`, `bugshot_workflow.py`,
    `gallery_server.py`, `vizline_workflow.py`, `select-bind-address`,
    `static/` and `templates/`.
  - The same copies appear under `.codex-plugin/skills/`.

  AGENTS.md does not say that these copies are generated, or by what
  (presumably `scripts/build-plugin`). It also does not say that they must be
  committed after a rebuild.
- `AGENTS.md:72-83` "Documentation Sync Rules" has five bullets, all
  targeting `skills/bugshot/SKILL.md` or the bugshot overlays.
- `ls /home/ketan/project/bugshot/README*` returns "No such file or directory".

Source: Shatter audit 2026-09-22 finding plugins-20.

## Acceptance criteria

- [ ] Project Structure lists all four skills (`skills/<name>/SKILL.md`),
      the viz/wire CLI and workflow modules, and the shared modules
      `capture_runner.py`, `image_diff.py` and `baseline_manifest.py`, with
      one line each. It states which copies under `skills/*/` and
      `.codex-plugin/skills/` are generated, and by which command.
- [ ] Documentation Sync Rules map each shared module or flag surface to the
      SKILL.md files it affects. At minimum:
      `capture_runner.py`/`wire_bugshot_*` flags -> `skills/wire-bugshot/SKILL.md`
      (and vizline);
      `image_diff.py` recognized extensions/thresholds -> `vizdiff` and
      `vizline` SKILL.md;
      `baseline_manifest.py` schema -> `vizline`/`vizdiff` SKILL.md.
      `ansi_render.py`, `bugshot_workflow.py`, `gallery_server.py`,
      `static/` and `templates/` -> the bugshot SKILL.md plus the vizdiff
      copies. They also include a rule to rebuild **and commit** the
      generated copies after editing a shared module.
- [ ] A short `README.md` exists at the repo root: purpose, install (link to
      INSTALL.md), the four skills with one line each, and where to find
      AGENTS.md.
- [ ] Proof at close: add a pytest, for example
      `tests/test_agents_md_coverage.py`. It fails when AGENTS.md is missing
      the exact relative path `skills/<name>/SKILL.md` for any directory under
      `skills/`, or the exact filename of any top-level `*.py` module. Match
      whole literal strings, not basenames: `vizdiff` already appears in
      AGENTS.md outside a skill entry, so a basename match would pass without
      the fix. In shell terms, the check is equivalent to
      `for d in skills/*/; do grep -qF "${d}SKILL.md" AGENTS.md || { echo MISSING ${d}SKILL.md; fail=1; }; done; exit ${fail:-0}`.
      Paste the test failing on the pre-change AGENTS.md (it should list at
      least `skills/vizline/SKILL.md`, `skills/vizdiff/SKILL.md`,
      `skills/wire-bugshot/SKILL.md` and `vizline_cli.py`) and passing after
      the change.
- [ ] If the published plugin payload (see the cache-bloat investigation)
      should not ship README/AGENTS.md, confirm that `scripts/build-plugin`
      excludes them. Otherwise nothing is needed.

## Suggested approach

Read `scripts/build-plugin` to confirm which files it copies into each skill
dir. Then extend the two AGENTS.md sections and write a README of about 30
lines. Keep the RTK block (`<!-- headroom:rtk-instructions -->`) untouched,
because it is tool-managed.

## Out of scope

- Documenting ANSI (`.ansi`) capture support and the `wire-bugshot --kind cli`
  template. That belongs to **bgs-3tq**; this issue only makes sure
  AGENTS.md's structure and sync rules cover the modules involved.
- Any behavioural change to the skills or modules.

## Metadata

- Priority: P3
- Type: chore
- Labels: audit-2026-09-22, docs
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: none (related: bgs-3tq)
