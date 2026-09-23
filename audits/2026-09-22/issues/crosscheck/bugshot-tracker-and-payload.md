# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback; Codex failed identity validation)

Bundle: audits/2026-09-22/issues/bugshot/bugshot-tracker-and-payload/BUNDLE.md
Reviewer: independent Claude reviewer, read-only. Claims were checked against /home/ketan/project/bugshot (HEAD e622d73), its bd database, the installed plugin cache, and the shatter tracker.

## Verified claims

- 01: `bd list --all --json` gives 115 issues (bgs 60, bugshot 55). Mirror statuses: 50 closed, 4 open, 1 deferred. Every mirror has a same-suffix bgs twin with an identical title, and each twin's status matches its mirror's. The ten mirrors with edges and their (dependency, dependent) counts match the draft exactly. Every mirror has comment_count 0.
- 02: bgs-3tq is P3 OPEN (created and updated 2026-09-05). str-qwua7.53 is P2 OPEN (created 2026-09-07).
- 03/04: The marketplace entry at marketplace.json:47-53 is `{"source":"github","repo":"ketang/bugshot"}`. The cache is 125M. The node_modules mtime of 2026-06-17 12:11:26 -0500 matches lastUpdated 2026-06-17T17:11:26.473Z. gitCommitSha is 4fb4d82, which is an ancestor of f8bf685. The version is 1.0.20 in both files. node_modules is gitignored (.gitignore:7). The cache contains .beads, tests, docs, .claude, .codex-plugin, __pycache__ and .gitignore. bgs-3cz is CLOSED P2 with the stated close reason and no comments.
- 05: AGENTS.md has "Project Structure" at line 37 and "Documentation Sync Rules" at line 72. Its content is as described. There is no README. All of the listed viz/wire modules and skill dirs exist.

## Findings

### MAJOR: 01 close-bugshot-mirror-duplicates: its dependency "re-pointing" step misdescribes the graph and could corrupt it
Every dependency edge on a mirror is mirror-to-mirror. For example, bugshot-hx3 -> bugshot-6zc and bugshot-7g0, and bugshot-qh9 -> bugshot-7p0 and bugshot-egh. The bgs twins already carry the identical bgs-to-bgs edges (bgs-hx3 -> bgs-7g0 and bgs-6zc; bgs-qh9 -> bgs-7p0 and bgs-egh). No edge crosses prefixes. Suggested-approach step 2 says "run `bd dep list bugshot-XXX`, add the same edge on the `bgs-*` twin". Read literally, that adds bgs -> bugshot cross-prefix edges or duplicates of edges that already exist, and then the task removes edges the twins do not need. The draft should say that twin edges already exist. The work is then to verify parity and close or delete the mirrors. The only concern is whether bd refuses to delete, or cascades on, mirrors that have mirror dependents.

### MINOR: 01: the "carry state only the mirror has" AC is effectively already satisfied
Titles and statuses match across all 55 pairs. Say so, and narrow the AC to a diff of description, priority and assignee, so the agent does not re-derive a no-op.

### MINOR: 01: the root-cause pointer is weak
.beads/config.yaml has only the bd-init commit (fb7977a) and no prefix history. The mirrors are present in issues.jsonl from at least 616affd (2026-06-11). Point the agent at the git history of .beads/issues.jsonl and the embeddeddolt import, not at config.yaml.

### MINOR: 03 installed-cache-bloat-investigation: the reinstall AC would destroy the evidence and disrupt the live install
AC 1 prescribes `claude plugin uninstall/install` on the operator's real cache. That erases the mtime and SHA evidence being traced. Suggested-approach step 2 (a clean CLAUDE_CONFIG_DIR or temp HOME) is the safer method, so make it the AC method and snapshot the current cache metadata first.

### MINOR: 03: an alternative node_modules source is not listed
The primary checkout /home/ketan/project/bugshot has a node_modules/ dir. Hypothesis (b)/(c) should explicitly include "installer copied a local working tree", alongside an npm step run at install time.

### MINOR: 05 agents-md-structure-and-readme: the shared-module copy inventory is incomplete
skills/vizdiff/ also holds copies of ansi_render.py, bugshot_workflow.py, gallery_server.py, vizline_workflow.py, select-bind-address, static/ and templates/. So bugshot core modules fan out into vizdiff too, and the sync rules need that mapping. The copies are also git-tracked, so the rule should say "rebuild and commit".

### MINOR: 05: the proposed mechanical check is trivially satisfied for skill dirs
In `for f in skills/*/ *.py`, `basename skills/vizline/` is `vizline`, a substring that appears in AGENTS.md via `vizline_cli.py` once modules are listed. Check `skills/$name/SKILL.md` literally instead.

## Verdict

The bundle is accurate: every quantitative claim re-verified. It is nearly ready to file. 02 and 04 are ready as-is. 05 and 03 are fileable with the minor fixes. 01 should be corrected before filing. Top fixes:
1. 01: Replace the edge re-pointing step and AC with "twin edges already exist; verify parity, then close or delete mirrors". Warn against adding cross-prefix edges.
2. 03: Make the clean-config-dir reinstall the AC method, and preserve the current cache metadata before any reinstall.
3. 05: Complete the vizdiff copy inventory and fix the mechanical check to match SKILL.md paths.
