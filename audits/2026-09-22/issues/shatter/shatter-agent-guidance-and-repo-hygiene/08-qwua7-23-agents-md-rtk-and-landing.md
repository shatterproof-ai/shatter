---
slug: qwua7-23-agents-md-rtk-and-landing
kind: note-to-existing
title: "Note on str-qwua7.23: etiquette rules inside the rtk-managed block, rtk 'always safe' text, landing prose contradicting land.py, merged remote branches (bd sync -> D4 Dolt remote)"
priority: P2
type: task
labels: [agents, docs]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.23
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.23: rtk block, landing prose, merged branches

Target: **str-qwua7.23** (open, P2, "Cut AGENTS.md by ~40%, delegate
procedures to bento skills, pin a byte-budget test"). Action: post ONE comment
with `bd comments add str-qwua7.23` using the text below. This combines the
old drafts `shatter-agent/17` and `shatter-docs-ui/30`.

## Comment text

> Audit 2026-09-22 re-confirms this issue's scope and adds four items.
> Line numbers were re-verified 2026-09-23 at origin/main 70465921.
> Evidence: `audits/2026-09-22/findings.json` plugins-12, agent-repo-09,
> agent-repo-12; `audits/2026-09-22/areas/{plugins-guidance,agent-repo}.md`.
>
> **1. Project rules still live inside the rtk-managed block.**
> `AGENTS.md:528` opens `<!-- headroom:rtk-instructions -->`. `:535`
> `## Shared-Machine Resource Etiquette (str-35vtk.5)` (heavyweight slots,
> cargo/go parallelism budgets, gate timing) sits inside it, and `:594` closes
> it. The dotfiles guidance (`codex/AGENTS.md:10-12`) says rtk's tooling
> manages these markers per repo, so an rtk re-sync would probably delete the
> etiquette rules. (That overwrite was inferred from the marker semantics and
> not executed.) Acceptance addition: the etiquette section sits outside the
> markers, next to the gate docs.
>
> **2. The rtk block text contradicts the tool-precedence rules.** `:531`
> "When running shell commands, **always prefix with `rtk`**" and `:533` "it
> is always safe to use". Project memory records rtk corrupting file
> redirects and serving stale git ref data. The global rule says dedicated
> Read/Grep tools come before any shell command. `Key Commands` (`:560`)
> advertises `rtk cargo test` (a bare test command) and `Rules` (`:590`)
> shows `rtk git add .`. The dotfiles claim that "the RTK precedence rule
> lives inside the markers" is false for this repo: the only "precedence" in
> AGENTS.md is `:73`, about `CARGO_TARGET_DIR`. Acceptance addition: after
> moving the etiquette section out, the block body (or a project section just
> above it) states the precedence: dedicated tools first; task facade over
> bare test commands; rtk optional and never for `find` with predicates,
> file redirects or landing-gating git ref reads. Coordinate the block-template
> wording with the dotfiles/rtk template owner (dotfiles #3 and #10 are closed
> but the text persists here, in bugshot `AGENTS.md:89` and in shatter-agents
> `AGENTS.md:36`).
>
> **3. Landing prose contradicts the shipped bento `land.py` driver.**
> - `AGENTS.md:104-133` "Landing the Plane" prescribes
>   `git push --force-with-lease` (`:118`), `git checkout main` (`:119`) and
>   `git merge --no-ff <issue-branch>` (`:121`).
> - `:125` ends with "`bd sync` once here so the landing yields a single sync
>   commit".
> - `:255-285` "Git Workflow" repeats the manual merge and cleanup.
> - `:340-369` "Beads Sync Cadence" is built around `bd sync`.
> - `:370-375` allows a "transient" `core.hooksPath` bypass.
> - `:513` says "**Never** run `git worktree remove`".
>
> bento land-work (`land.py`: lease, preview worktree, verifier, teardown) is
> the actual path. `bd sync` does not exist in bd 1.1.0.
>
> Under maintainer decision **D4 (2026-09-23)**, the beads JSONL import and
> `bd sync` are retired and cross-machine tracker sync moves to a Dolt remote.
> Replacement landing text should therefore say:
> - `bd` state changes (claim/update/close) write the local Dolt DB;
> - landing does **not** commit `.beads/*.jsonl`;
> - tracker state is shared with `bd dolt push` / `bd dolt pull` against the
>   Dolt remote configured by the new audit issue
>   `beads-retire-jsonl-import-dolt-remote`, following the procedure that
>   issue records once in AGENTS.md.
>
> Do not add a `bd sync` step, a JSONL-export commit, a hook-timeout env var,
> or any hook-bypass exception. Drop the `:370-375` "transiently bypass"
> clause. The line-level `bd sync` removal across AGENTS.md, `.beads/PRIME.md`
> and skills is owned by `beads-jsonl-consumers-drop-bd-sync`. This issue owns
> replacing the landing sections with a pointer to `bento:land-work` plus the
> shatter-specific inputs (verifier manifest, `task affected` /
> `task check` gates, the Dolt-remote sync step).
>
> **4. Merged remote branches pile up.** `git for-each-ref --merged origin/main refs/remotes/origin`
> -> 35 merged branches, of 66 remote branches, per the local fetch state.
> AGENTS.md calls cleanup "mandatory", but nothing runs it. Four unmerged
> branches (`origin/str-qwua7.4-testplan-http-body-fix`,
> `.7-protocol-registry-validate`, `.16-restore-bd-dolt`,
> `.17-stale-claims-cleanup`) contain the stray fixture commit `e50fc399`;
> their review and deletion are tracked by the new audit issue
> `fixture-corruption-incident-reverify`. Acceptance addition: run
> `scripts/cleanup-merged-remote-branches.sh` (dry-run, operator review, then
> `--execute`) once after the landing prose is replaced. Its in-progress
> protection currently reads the stale `.beads/issues.jsonl`
> (`AGENTS.md:274-276`), so run it only after
> `beads-jsonl-consumers-drop-bd-sync` has moved it off the JSONL, or review
> the list by hand against `bd list --status in_progress`.
>
> Size: AGENTS.md is still 30,731 bytes (+ CLAUDE.md 8,617 = 39,348), so this
> issue's 12 KB / 20 KB budget is unchanged.
