# fail-closed guidance: validators must fail on empty extraction and carry a canary test

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P3
- labels: documentation
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: protocol-parity-21

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`~/dotfiles/docs/code-writing-guidance/fail-closed-defaults.md` covers "a tool that is not installed must fail", missing allowlists and missing expiry. It does not cover validators that pass vacuously. In Shatter, three controls were green for months while checking nothing:

- `scripts/validate-protocol-registry.py`: TypeScript extraction returns empty sets, which are silently skipped.
- The conformance `known_drifts` patterns are regexes matched as substrings, so they can never match.
- The JSON schema checks see only hand-written fixtures.

## Acceptance criteria

- [ ] `fail-closed-defaults.md` (or `validation-and-errors.md`) contains a rule. Any gate or validator that extracts facts from source must fail when an expected extraction is empty or a declared pattern never matches. It must also carry a mutation or canary test that shows it goes red on a seeded defect.
- [ ] The rule includes one short example.

## Source

Shatter audit 2026-09-22 finding protocol-parity-21 (`areas/protocol-parity.md`).
