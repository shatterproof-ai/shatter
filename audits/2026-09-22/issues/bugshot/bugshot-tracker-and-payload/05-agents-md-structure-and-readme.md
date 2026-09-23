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
- Shared modules are copied into skill dirs: `skills/{vizline,vizdiff}/` hold
  `capture_runner.py`, `image_diff.py` and `baseline_manifest.py`, and
  `skills/wire-bugshot/` holds `image_diff.py` and `baseline_manifest.py`.
  The same copies appear under `.codex-plugin/skills/`. AGENTS.md does not say
  these copies are generated, or by what (presumably `scripts/build-plugin`).
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
      They also include a rule to rebuild the generated copies after editing a
      shared module.
- [ ] A short `README.md` exists at the repo root: purpose, install (link to
      INSTALL.md), the four skills with one line each, and where to find
      AGENTS.md.
- [ ] Proof at close: a mechanical check, run and pasted into the close
      reason, showing that every `skills/*/SKILL.md` and every top-level
      `*.py` module is named in AGENTS.md. For example
      `for f in skills/*/ *.py; do grep -q "$(basename $f)" AGENTS.md || echo MISSING $f; done`
      printing nothing. Optionally add it as a pytest so future skills cannot
      be omitted.
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
