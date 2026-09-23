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

## Evidence (re-verified 2026-09-23 at 56c86168)

- `protocol/PARITY.md:76-78`, under "### Rust", says "No complex type capabilities are implemented yet." The registry, matrix and `protocol/conformance/golden/handshake/rust.json` all list Rust `uuid, url, date, date_time`.
- The Go complex-type table (`PARITY.md:62-74`) omits `go_byte`, which the matrix marks `go: supported` (`parity-matrix.yaml:458`) and `golden/handshake/go.json` advertises. The prose at `:45` cites `rune` as an ecosystem-specific Go type, but the matrix marks `rune` `not_supported` for every frontend (`:449-456`). The command table (`:29-39`) omits `get_invocation_plan`, which is in the registry, the matrix (`parity-matrix.yaml:175`, Go implemented) and the Go handshake golden.
- The `adapter-owned-instrumentation-coverage-partial` block says `**Affected frontends:** go, rust` (`PARITY.md:283`). The matrix entry (`parity-matrix.yaml:1118`) has `[rust]`, because Go was fixed by str-1qd5i.
- `PARITY.md:139`: "All 11 error codes defined in `registry.yaml`". The validator reports `Registry: 10 commands, 11 statuses, 12 error codes`.
- `scripts/validate-parity.py:419-445` `parity_md_divergence_ids` compares heading IDs only.
- Audit finding protocol-parity-06 (confirmed, P2).

## Acceptance criteria

- [ ] PARITY.md's capability tables and divergence blocks are generated from `protocol/parity-matrix.yaml` by a generator whose `--check` mode runs in `task parity`. Alternatively, the tables and the divergence mirror are deleted and PARITY.md links to the matrix. The close note records which.
- [ ] All errors listed above are gone.
- [ ] If generation is chosen, it reuses the str-qwua7.24 / capability-single-source generator rather than adding a new one.
- [ ] Proof at close: a canary matrix edit makes `task parity` fail when forced to execute (generation option), or `git grep` shows no remaining hand-copied tables (deletion option). Paste the output.

## Suggested approach

Deletion plus a link is the cheaper and more durable option unless someone reads PARITY.md offline. If generation is kept, delimit the generated regions with markers so the narrative prose stays hand-written.

## Out of scope

The root-level `PARITY.md` (str-qwua7.45) and the crate `CLAUDE.md` and skill tables (str-qwua7.24).

## Dependencies

- Blocked by: none.
- Related: capability-single-source, str-qwua7.24, str-qwua7.45, parity-guidance-skill-and-template.

Size: S. Priority: P2. Type: bug. Labels: parity, protocol, docs, audit. Parent: Epic: Audit 2026-09-22 findings.
