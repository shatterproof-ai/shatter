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

Part of #<epic>. Priority: P3. Type: enhancement.

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

- [ ] `fail-closed-defaults.md` (or `validation-and-errors.md`, with a pointer from the other) states two rules for any gate or validator that extracts facts from source:
  - It must fail when an expected extraction is empty, or when a declared pattern, allowlist entry or known-drift entry never matches.
  - It must carry a canary or mutation test showing that it goes red on a seeded defect.
- [ ] The rule includes one short example, for example: "a registry validator that finds 0 message types in a frontend fails; its test deletes one handler and asserts the validator reports it."
- [ ] Proof at close: the closing comment shows the output of `grep -n "canary\|empty extraction" ~/dotfiles/docs/code-writing-guidance/*.md`.

## Out of scope

Fixing the Shatter validators themselves. That is tracked in the shatter protocol-parity bucket.

## Dependencies

None.

## Source

Shatter audit 2026-09-22 finding protocol-parity-21, in `areas/protocol-parity.md`.
