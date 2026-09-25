---
slug: validators-fail-on-empty-extraction
kind: new
title: "Fail-closed guidance does not cover vacuous validators: require failure on empty extraction and a canary test"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Fail-closed guidance does not cover vacuous validators: require failure on empty extraction and a canary test

Part of #<epic>. Priority: P3. Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

`~/dotfiles/docs/code-writing-guidance/fail-closed-defaults.md` covers three cases: an uninstalled tool must fail, an empty allowlist means deny, and a missing expiry means reject. It says nothing about validators that pass vacuously, meaning they extract nothing and report green. Agents are told to make gates pass, but nothing tells them to prove that a gate can fail.

## Evidence

- dotfiles @ `81f35e1`: `fail-closed-defaults.md` is 6 lines long, and `grep -in "empty\|canary\|mutation"` matches only line 4, the allowlist and expiry rule. `validation-and-errors.md:6` only points back to it.
- In the Shatter repo, three separate controls stayed green for months while checking nothing:
  - `scripts/validate-protocol-registry.py`: TypeScript extraction returns empty sets, which are silently skipped.
  - Conformance `known_drifts` patterns are written as regexes but matched as substrings, so they can never match.
  - JSON schema checks only ever see hand-written fixtures.

  The shatter-side fixes are tracked in the shatter protocol-parity bucket.

## Acceptance criteria

- [ ] `fail-closed-defaults.md` (or `validation-and-errors.md`, with a pointer from the other) states these rules for any gate or validator that extracts facts from source:
  - It fails when an extraction that the gate's contract expects to be non-empty returns nothing (for example, a frontend that must declare message types yields zero).
  - Declarations whose contract says "this specific thing exists", such as known-drift entries, expected-failure entries or suppressions of a named current defect, are reported as stale when they no longer match anything. The gate fails or warns on stale entries, as the gate's own docs define.
  - Ordinary allowlist or permit entries are not required to match anything in a given repository or run. An entry that matches nothing is valid, and the rule must say so, so that implementers do not add false failures.
  - It carries a canary or mutation test showing that it goes red on a seeded defect.
- [ ] The text includes two short examples: a registry validator that finds 0 message types in a frontend fails, and its test deletes one handler and asserts the validator reports it; and an allowlist entry for a path absent from this repo is accepted without error.
- [ ] The existing rule "An empty allowlist means deny" (`fail-closed-defaults.md:4`) is left unchanged, and the new text does not contradict it.

## Proof at close

The closing comment quotes the new rule text and shows `grep -n "canary\|empty extraction\|stale" ~/dotfiles/docs/code-writing-guidance/*.md`.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Fixing the Shatter validators themselves. That is tracked in the shatter protocol-parity bucket.

## Dependencies

None.

## Source

Shatter audit 2026-09-22 finding protocol-parity-21, in `areas/protocol-parity.md`.
