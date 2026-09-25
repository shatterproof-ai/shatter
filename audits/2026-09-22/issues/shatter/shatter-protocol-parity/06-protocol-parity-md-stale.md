---
slug: protocol-parity-md-stale
kind: new
title: "protocol/PARITY.md hand-mirrors the parity matrix and is stale; the validator compares only divergence IDs"
priority: P2
type: bug
labels: [parity, protocol, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# protocol/PARITY.md hand-mirrors the parity matrix and is stale; the validator compares only divergence IDs

## Problem

`protocol/PARITY.md` hand-copies the capability tables and divergence blocks from `protocol/parity-matrix.yaml`. `validate-parity.py` checks only that the divergence heading IDs match, so content drift is invisible, and several statements are now false. Agents and contributors read PARITY.md as the human-facing parity contract.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- `protocol/PARITY.md:76-78`, under "### Rust", says "No complex type capabilities are implemented yet." The registry, matrix and `protocol/conformance/golden/handshake/rust.json` all list Rust `uuid, url, date, date_time`.
- The Go complex-type table (`PARITY.md:62-74`) omits `go_byte`, which the matrix marks `go: supported` (`parity-matrix.yaml:458`) and `golden/handshake/go.json` advertises. The prose at `:45` cites `rune` as an ecosystem-specific Go type, but the matrix marks `rune` `not_supported` for every frontend (`:449-456`). The command table (`:29-39`) omits `get_invocation_plan`, which is in the registry, the matrix (`parity-matrix.yaml:175`, Go implemented) and the Go handshake golden.
- The `adapter-owned-instrumentation-coverage-partial` block says `**Affected frontends:** go, rust` (`PARITY.md:283`). The matrix entry (`parity-matrix.yaml:1118`) has `[rust]`, because Go was fixed by str-1qd5i.
- `PARITY.md:139`: "All 11 error codes defined in `registry.yaml`". The validator reports `Registry: 10 commands, 11 statuses, 12 error codes`.
- `scripts/validate-parity.py:419` `parity_md_divergence_ids` extracts heading IDs only, and `validate_divergence_metadata()` (`:445`, sync block `:577-596`) errors when the ID sets differ. Content under the headings is never compared.
- `Taskfile.yml:252-261` `parity.sources` omits `protocol/PARITY.md`, `protocol/parity-matrix.yaml` and `scripts/validate-parity.py`, so an edit to PARITY.md alone is answered by a cached pass.
- Audit finding protocol-parity-06 (confirmed, P2).

## Acceptance criteria

- [ ] The close note records which option was taken:
  - **Generate:** PARITY.md's capability tables and divergence blocks are generated from `protocol/parity-matrix.yaml` into marked regions, by a generator whose `--check` mode runs in `task parity`. It reuses the str-qwua7.24 table generator (extend it; do not add a second one). The existing heading-ID sync in `validate_divergence_metadata()` may stay or be subsumed by the `--check`.
  - **Delete:** the tables and the divergence mirror are deleted and PARITY.md links to the matrix. `validate_divergence_metadata()` (`scripts/validate-parity.py:445`, PARITY.md sync at `:577-596`) currently hard-fails when a matrix divergence ID has no `### <id>` heading in PARITY.md, so deletion must also remove or replace that check (for example, assert that PARITY.md contains the link and no `### ` divergence headings), and update `scripts/test_validate_parity.py` to match.
- [ ] Every error listed under Evidence is gone.
- [ ] Proof at close (generate): edit the matrix only (for example change the affected frontends of `adapter-owned-instrumentation-coverage-partial`), then paste the **pre-change** `task parity` exit 0 on that edit (today only heading IDs are compared) and the post-change failure. Revert.
- [ ] Proof at close (delete): paste `task parity` passing with the mirror removed, the updated validator test run, and the output of `git grep -nE '^### .[a-z0-9-]+.$' protocol/PARITY.md` showing that no divergence headings remain.
- [ ] Cache wiring: `protocol/PARITY.md`, `protocol/parity-matrix.yaml` and `scripts/validate-parity.py` are in `parity.sources` (task-sources-cover-real-inputs adds them; add them here if it has not landed). Proof: `touch protocol/PARITY.md && task parity` (ordinary invocation, no `--force`) executes the validator rather than printing `is up to date`; paste the output.

## Suggested approach

Deletion plus a link is the cheaper and more durable option unless someone reads PARITY.md offline. If generation is kept, delimit the generated regions with markers so the narrative prose stays hand-written.

## Out of scope

The root-level `PARITY.md` (str-qwua7.45) and the crate `CLAUDE.md` and skill tables (str-qwua7.24).

## Dependencies

- Blocked by: none.
- Related: capability-single-source, str-qwua7.24 (open; the generator to extend), str-qwua7.45 (open; root-level PARITY.md), parity-guidance-skill-and-template, task-sources-cover-real-inputs (shatter-gates-integrity bucket).

Size: S. Priority: P2. Type: bug. Labels: parity, protocol, docs, audit. Parent: Epic: Audit 2026-09-22 findings.
