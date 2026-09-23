---
slug: ss-yoa-reopen-note
kind: reopen-note
title: "Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-ups are clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding"
priority: P2
type: task
labels: [inventory, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors, go-cobra-extractor, coverage-unextracted-language-finding]
existing_id: ss-yoa
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md (comments are writes). File clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding first so their real ids replace the placeholders. blocked_by here is a filing-order dependency for placeholder substitution only; do not add it as a tracker dependency on the closed ss-yoa."
---

# Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-ups are clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding

**Target:** `ss-yoa` (CLOSED, P2, feature, "Support skill/markdown repos: a
skill: surface prefix or doc-directory extractor (current extractors are
TS/JS-only)". Close reason: "4083924b42050d1d7cd882a5966b56b778e25d09 landed
on main (skill: prefix + skill-dir extractor + audit resolution + generator
ref guard + markdown-only round-trip test)"). Re-verified with
`bd show ss-yoa` on 2026-09-23.

**Action:** add the comment below with `bd comments add ss-yoa ...`. **Do
not reopen ss-yoa.** Its stated scope (skill/markdown repos) was delivered.
The remaining gap is tracked in the new issues. The filer replaces each
`<slug>` placeholder with the real id.

## Comment text

**Audit 2026-09-22 note (shatter audit, finding plugins-05)**

The skill/markdown work in this issue landed and works: 15 `skill` surfaces
are found on the shatter repo. The broader problem named in this issue's
title, "current extractors are TS/JS-only", is still there for CLI
surfaces. On 2026-09-23 at storystore HEAD cca768d:

```
python3 shared/inventory.py --repo-root <shatter checkout>
-> kinds {test: 2664, heading: 73, skill: 15, bin: 1}; no cli-command
-> languages {detected: [go, javascript, rust, typescript], extracted: [javascript, typescript]}
```

The only CLI extractor is the commander.js regex at
`shared/inventory.py:140` (`_CLI_COMMAND_RE`). Shatter's clap derive
subcommands (`shatter-cli/src/args.rs:1165`) are not found, so
`stories-coverage` reports no uncovered CLI surfaces on shatter.

Follow-ups (this issue stays closed):
- <clap-cobra-extractors>: Rust clap derive extraction, nested commands,
  per-kind language metadata, spec.md contract update.
- <go-cobra-extractor>: Go cobra extraction.
- <coverage-unextracted-language-finding>: report a detected language with
  no CLI extractor as a counted coverage finding.
