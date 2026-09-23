---
slug: qwua7-52-story-seed-list
kind: note-to-existing
title: "Note on str-qwua7.52: proposed journey-level seed stories, and the storystore inventory finds no clap CLI surfaces"
priority: P2
type: task
labels: [docs, stories, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.52
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.52 (open, P2: "Adopt storystore: initialise docs/stories, seed CLI stories, land str-u394l.3")

Target: `str-qwua7.52`. Post the text below as a comment with `bd comments add str-qwua7.52`. Do not change the issue's status or priority.

## Comment text

> **Audit 2026-09-22 (findings docs-11, goals-19): still unstarted, plus a proposed seed list.**
>
> `docs/stories/` still does not exist (`ls docs/stories` gives ENOENT at 56c86168). This issue has had no activity since the 2026-09-06 "Adopt" decision, and drift-patrol's `docs-stories` slot is still PENDING under str-u394l.3. Several 2026-09-22 findings are journey-level breakages that no per-command test catches: goals-01, 02 and 04; sandbox safety; the regression baseline; the resume layout. Story-level evidence checks are meant to surface exactly these.
>
> **Proposed seed stories.** This list is the auditor's proposal, not a maintainer decision. Trim or reorder freely. Each should cite existing evidence (walkthrough.yaml steps, e2e tests) and have an executable check.
>
> 1. **First exploration.** `shatter explore` on one function, then read the report (TS, Go and Rust).
> 2. **Safe execution of targets.** The default-deny host-write guard and the sandbox remedies, per frontend.
> 3. **Baseline, then change, then detect** (the core regression journey). Save a baseline with `explore --spec-out`, change the code, then detect the regression with `spec-diff` / `stale` locally and in CI. Per decision D2 (2026-09-23), spec-diff is the regression tool and snapshot `shatter diff` is being retired, so this story must not cite `shatter diff`.
> 4. **CI scan with failure thresholds.** `scan --fail-on-failures`, reports and exit codes.
> 5. **Resume an interrupted scan.** `--resume`; the checkpoint and the artifact layout.
> 6. **Functions that use live resources** (docs/resource-parameters.md).
> 7. **Offline staged pipeline.** analyze, then solve, then observe.
> 8. **Cross-language compare.** Compare the TS and Go implementations of one function.
> 9. **init / config / doctor.** First-time project setup.
> 10. **Agent-driven usage** through the shatter-agents plugin (run-shatter, interpret-shatter-spec).
> 11. **HTML report review.**
>
> **Tooling limitation (not a blocker for this issue or for str-u394l.3's basic gate).** storystore's inventory currently finds **0 `cli-command` surfaces** in shatter. Its only CLI extractor matches commander.js `.command('name')`. On this repo it detects go, javascript, rust and typescript but extracts only javascript and typescript, so Shatter's clap subcommands in `shatter-cli/src/args.rs` are invisible to it. The consequence is narrow: automatic *CLI-surface completeness* reporting from `stories-coverage` will show nothing uncovered until the storystore issue **clap-cobra-extractors** (storystore epic "Audit 2026-09-22 findings (storystore)") lands. Everything else can proceed now: `stories-init`, seeding, and str-u394l.3's recorded acceptance (directory and INDEX exist, the index is fresh, active stories carry evidence fields, patrol wiring). drift-patrol's `check_docs_stories` (`scripts/drift-patrol.py:453`ff.) already checks the index without any CLI extraction. Until the extractor lands, list the clap subcommands manually in the stories README if CLI completeness is wanted.
>
> Also link `docs/stories/INDEX.md` from `docs/INDEX.md` when it exists, as this issue already requires.

## Why a note and not a new issue

str-qwua7.52 already owns stories-init, seeding, the INDEX link and landing str-u394l.3. The audit adds a concrete journey list and records one external tooling limitation that affects only automatic CLI-surface completeness.

## Source

Audit 2026-09-22, findings docs-11 (confirmed, P2) and goals-19 (confirmed, P3). Both are duplicate-open of str-qwua7.52. Related: str-u394l.3, and the storystore clap-cobra-extractors issue (plugins-05).
