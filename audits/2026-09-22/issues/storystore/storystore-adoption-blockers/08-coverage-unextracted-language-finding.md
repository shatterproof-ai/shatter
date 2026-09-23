---
slug: coverage-unextracted-language-finding
kind: new
title: "stories-coverage: report a detected language with no cli-command extractor as a counted finding (text and JSON), including under --thorough"
priority: P2
type: feature
labels: [coverage, adoption, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. File clap-cobra-extractors first so its real id replaces the blocked_by slug."
---

# stories-coverage: report a detected language with no cli-command extractor as a counted finding (text and JSON), including under --thorough

## Problem

When a repo's CLI is written in a language storystore cannot extract,
`stories-coverage` reports zero uncovered CLI surfaces, which reads as "fully
covered". The only signal today is advisory header text
(`shared/coverage.py:685-690`: "Note: go, rust detected but not covered by
built-in extractors. Re-run with --thorough ..."). That note is not a
finding: it is suppressed under `--thorough` (`if uncovered and not
thorough`), the "## Findings (N)" count ignores it, and it is absent from
JSON output, so a script checking findings sees an empty CLI result. On
shatter today this hides every clap subcommand; after clap-cobra-extractors
it would still hide any Go CLI surface in a repo without cobra support.
Split out of the pre-revision clap-cobra-extractors draft during the
2026-09-23 cross-check revision.

## Evidence (storystore HEAD cca768d)

- `shared/coverage.py:683-690`: the Language Coverage block and the
  `not thorough`-gated note.
- `shared/coverage.py:69`: `DEFAULT_SURFACE_KINDS` includes `cli-command`.
- `spec.md:385-392`: the header "suggests `--thorough`"; no finding kind is
  specified for the gap.
- Source finding: plugins-05.

## Acceptance criteria

- [ ] When `cli-command` is among the requested surface kinds and a detected
      language has no `cli-command` extractor (per the per-kind metadata
      from clap-cobra-extractors), the report emits a finding of a new,
      documented kind (e.g. `surface-kind-unextracted`, naming the language
      and surface kind). It is counted in "## Findings (N)" and present in
      the JSON output.
- [ ] The finding is still emitted under `--thorough` unless the agent
      supplied inferred surfaces of that kind for that language.
- [ ] No finding when `cli-command` is not requested, or when every detected
      language has a `cli-command` extractor.
- [ ] Tests in `tests/test_storystore_coverage.py` cover each of the three cases above
      with fixtures, for both text and JSON output. They fail at the
      pre-change commit and pass after; paste both runs.
- [ ] The new finding kind is documented in `spec.md` and `shared/spec.md`
      next to the other coverage findings, and in the stories-coverage
      SKILL.md if it lists finding kinds.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Out of scope

- Writing any new extractor.
- The same treatment for `http-route` / `schema` / `copy` (file separately
  if wanted; the mechanism should not preclude it).

## Priority

P2: a silent "nothing uncovered" on an unextractable CLI misleads adopters.

## Type / Labels

feature; coverage, adoption, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: clap-cobra-extractors (defines the per-kind language
  metadata this finding keys on).
