# Note for str-qwua7.23: rtk-managed block also carries 'always safe' rtk claim contradicting memory/global rules

- Priority: P2
- Type: note
- Labels: agents,docs
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: note-to-append (target str-qwua7.23)
- Source findings: plugins-12 (related L3: agent-repo-09)
- Action: append as notes to str-qwua7.23 (no new issue)
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
Audit 2026-09-22 (evidence `audits/2026-09-22/areas/plugins-guidance.md`, `audits/2026-09-22/areas/agent-repo.md`):

- Still present at HEAD: AGENTS.md:528 opens `<!-- headroom:rtk-instructions -->`,
  :535 `## Shared-Machine Resource Etiquette (str-35vtk.5)` sits inside the
  managed block, :594 closes it. An rtk re-sync would delete the etiquette rules.
- Additional item for this issue's acceptance: the block text says rtk "is
  always safe to use" and "always prefix with rtk" (AGENTS.md:531-533). This
  contradicts project memory (rtk corrupts file redirects and can serve stale
  git ref data; `head -N` in compound commands was displayed wrongly during
  this audit) and the global rule that dedicated Read/Grep tools take
  precedence. The dotfiles claim that the precedence rule lives inside the
  markers is false for this repo (the only "precedence" in AGENTS.md is
  line 73, about CARGO_TARGET_DIR).
- Suggested acceptance addition: after moving the etiquette section out, replace
  the block body with a precedence statement (dedicated tools first; task facade
  over bare test commands; rtk optional, never for find with predicates,
  redirects, or landing-gating git ref reads), coordinated with the dotfiles rtk
  template owner.
