# NOTE TO APPEND to str-qwua7.23: AGENTS.md landing prose contradicts land.py, and the etiquette section sits inside the rtk-managed block (new evidence)

- Priority: P2
- Type: (existing issue)
- Labels: (existing)
- Tracker action: note to append to str-qwua7.23 (`bd comments add`)
- Related: str-g18x, str-ly5bz, bento land-work
- Source findings: audit 2026-09-22 agent-repo-09, agent-repo-12 (confirmed)

<!-- body -->
Audit 2026-09-22 re-confirms this issue's scope and adds evidence:

1. **The rtk-managed block still swallows local rules.** `AGENTS.md:528` opens `<!-- headroom:rtk-instructions -->`, `:535` is `## Shared-Machine Resource Etiquette (str-35vtk.5)`, and `:594` closes the block. An rtk re-sync would delete the etiquette rules. The block also says "always prefix with `rtk`" (`:531`) and "always safe to use" (`:533`), which contradicts the global rule to prefer Read/Grep and the repo memory about rtk corrupting redirects. The upstream wording fix belongs to dotfiles and the rtk template (dotfiles #3/#10 claimed it); this repo should move its section now.
2. **The landing prose contradicts the shipped driver.** AGENTS.md:104-133 and :255-285 prescribe `git checkout main`, `git merge --no-ff`, `git push --force-with-lease`, `bd sync` (the command no longer exists in bd 1.1.0) and "Never run git worktree remove". bento `land.py` is the actual path.
3. **Merged branches pile up.** 38 remote branches are merged into origin/main but not deleted (`git branch -r --merged origin/main`). Four unmerged `origin/str-qwua7.*` branches (.4-testplan-http-body-fix, .7-protocol-registry-validate, .16-restore-bd-dolt, .17-stale-claims-cleanup) contain the stray fixture commit e50fc399 "init" and should be deleted once reviewed. Run `scripts/cleanup-merged-remote-branches.sh --execute` once after review.
4. AGENTS.md is 30,731 bytes, and no agent doc has changed since 09-07.

Extra acceptance: the etiquette section sits outside the headroom markers; the landing sections are replaced by a pointer to `bento:land-work` plus shatter-specific inputs; there are no `bd sync` references; and the remote branch debris is cleaned.
