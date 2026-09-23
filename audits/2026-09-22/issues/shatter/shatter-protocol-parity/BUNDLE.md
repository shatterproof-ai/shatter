# Bundle: shatter-protocol-parity

- Audit: 2026-09-22 Shatter audit, final issue drafts (nothing filed)
- Bucket: shatter-protocol-parity: protocol contracts and parity machinery (dispatch checks, conformance harness, single-source capabilities, schemas, governance and protocol docs, validator liveness)
- Repo: shatter. Tracker: bd in /home/ketan/project/shatter (prefix str). Parent epic: "Epic: Audit 2026-09-22 findings"
- Evidence re-verified against the audit worktree at 56c86168 on 2026-09-23
- Entries: 16 (13 new issues, 1 reopen-note, 1 note-to-existing, plus 1 split: capability-single-source was sized L, so the 13-registry-enum codegen (protocol-parity-19) is split out as protocol-codegen-all-registry-enums, P3)

## Maintainer decisions (2026-09-23)

- D1 Releases: KEEP Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- D2 shatter diff: RETIRE snapshot `shatter diff` and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to take it. Correct the shatter-agents `shatter diff --staged` docs.
- D3 Concolic positioning: MEASURE FIRST. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue (blocked by both) re-decides "concolic-first" positioning. No doc softening now.
- D4 Beads: RETIRE the JSONL import in shatter and sync the tracker through a Dolt remote. The first step verifies whether importing the stale JSONL has clobbered newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT env var and no hook-bypass guidance.
- D5 Git identity: the leaked [user] section was already removed. Add a .mailmap (test@example.com "Test"/"Test User" → Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a drift-patrol/setup-hooks git-state check, and .git/config snapshots in test_git_fixture_isolation.py.
- D6 Filing: after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

Decision touchpoints in this bucket: none of the entries carries a decision_ref. D4 is relevant only to divergence-tracking-issue-liveness, whose check must SKIP rather than judge liveness from the stale `.beads/issues.jsonl` export (see beads-jsonl-consumers-drop-bd-sync).

## Index

| File | Slug | Kind | Existing | P | Blocked by |
|---|---|---|---|---|---|
| 01-parity-dispatch-reconciliation.md | parity-dispatch-reconciliation | new | - | P2 | [] |
| 02-conformance-harness-correctness.md | conformance-harness-correctness | new | - | P2 | [] |
| 03-capability-single-source.md | capability-single-source | new | - | P2 | [] |
| 04-protocol-codegen-all-registry-enums.md | protocol-codegen-all-registry-enums | new | - | P3 | [] |
| 05-protocol-schemas-reject-real-output.md | protocol-schemas-reject-real-output | new | - | P2 | [] |
| 06-protocol-parity-md-stale.md | protocol-parity-md-stale | new | - | P2 | [] |
| 07-governance-md-omits-matrix.md | governance-md-omits-matrix | new | - | P2 | [] |
| 08-protocol-md-execute-fields.md | protocol-md-execute-fields | new | - | P2 | [] |
| 09-divergence-tracking-issue-liveness.md | divergence-tracking-issue-liveness | new | - | P2 | [] |
| 10-validator-ts-extraction-empty.md | validator-ts-extraction-empty | new | - | P2 | [validator-optional-command-warning] |
| 11-validator-reopen-note.md | validator-reopen-note | reopen-note | str-qwua7.7 | P2 | [] |
| 12-validator-optional-command-warning.md | validator-optional-command-warning | new | - | P3 | [] |
| 13-protocol-rs-doc-comments.md | protocol-rs-doc-comments | new | - | P3 | [] |
| 14-protocol-test-doubles-relocate.md | protocol-test-doubles-relocate | new | - | P3 | [] |
| 15-parity-guidance-skill-and-template.md | parity-guidance-skill-and-template | new | - | P3 | [] |
| 16-qwua7-37-premise.md | qwua7-37-premise | note-to-existing | str-qwua7.37 | P3 | [] |

---

<!-- file: 01-parity-dispatch-reconciliation.md -->

---
slug: parity-dispatch-reconciliation
kind: new
title: "Parity gates never check dispatch: a command can be advertised but not dispatched and still pass every static gate"
priority: P2
type: task
labels: [parity, protocol, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Parity gates never check dispatch: a command can be advertised but not dispatched and still pass every static gate

## Problem

`scripts/validate-parity.py` compares only the capability lists that each frontend advertises in its handshake. `scripts/validate-protocol-registry.py` only *warns* when a command is missing. Neither script checks that each command is dispatched. During the audit, a scratch-copy mutation removed `prepare` dispatch from all three frontends and left the handshake advertising it. Both validators still exited 0 ("Parity check passed."). The golden handshake files compare only the advertised list. The only `prepare` conformance case runs only on Rust. No gate owns the end-to-end claim that a command marked implemented is actually dispatched.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `scripts/validate-parity.py:294-386` has three detectors. `detect_typescript` reads `SUPPORTED_CAPABILITIES` from `handlers.ts` (:305-316). `detect_go` reads `CommandCapabilities` and `handleHandshake` (:321-353). `detect_rust` reads `handle_handshake` (:355-386). None of them parse the dispatch arms.
- `scripts/validate-protocol-registry.py:641-669` `validate()` skips empty source sets (`if not src_set: continue`, :647) and reports missing commands as `(may be unimplemented)` warnings (:664-666). The script exits 0: `python3 scripts/validate-protocol-registry.py` → `All checks passed (with informational warnings).`
- Dispatch sites that no gate reads: `shatter-ts/src/handlers.ts:556` (`case "prepare": {`), `shatter-go/protocol/handler.go:273` (`case "prepare":`), `shatter-rust/src/handler.rs:547` (`"prepare" => (self.handle_prepare(resp, req), false),`).
- `protocol/conformance/conformance_cases.yaml:167` `prepare_supported_rust` is the only prepare success case, with `frontends: [rust]`. The matrix (`protocol/parity-matrix.yaml:127`) marks prepare implemented for all three frontends.
- Audit finding protocol-parity-01. The verifier confirmed it from code without re-running the mutation, and downgraded it P1 → P2: this is a latent gate gap, and the E2E suites do exercise prepare at runtime.

## Acceptance criteria

- [ ] `validate-parity.py` extracts the dispatch arms for TS (`handlers.ts` switch), Go (`handler.go` switch) and Rust (`handler.rs` match). It hard-fails (non-zero exit) in both directions: (a) the matrix marks a command `implemented` for a frontend, or the handshake advertises it, but the command is not dispatched; (b) a command is dispatched but neither advertised nor listed in the matrix. The base-protocol commands `handshake` and `shutdown` are required for every frontend.
- [ ] An extractor that finds zero dispatch arms for a frontend is a hard error, never a silent pass.
- [ ] Every (frontend × command the matrix marks implemented) pair has at least one minimal runtime conformance case in `conformance_cases.yaml`, generated or hand-written. Hand-written cases need a test that fails when a pair has no case.
- [ ] Proof at close: a scripted mutation test (unit test in `scripts/test_validate_parity.py` or equivalent, operating on fixture copies) removes one dispatch arm per frontend while keeping the advertisement, and asserts the gate exits non-zero. Paste the failing-then-passing output into the close note.
- [ ] `task parity` and `task conformance` pass after being forced to execute (not checksum-cached); record the output.

## Suggested approach

Put the dispatch extractors in one shared helper module used by `validate-parity.py`. The TS/Go/Rust extractor work in validator-ts-extraction-empty can then reuse it rather than growing a second regex set. Prefer generating the per-command conformance cases from the matrix over hand-writing 3×N entries.

## Out of scope

- Fixing harness timeouts, known_drifts or the summary line (conformance-harness-correctness).
- Deriving the matrix, registry and golden copies from one source (capability-single-source).

## Dependencies

- Blocked by: none.
- Related: validator-ts-extraction-empty (shared TS dispatch extractor), conformance-harness-correctness (new cases run through the harness), str-qwua7.7 (closed; its dispatch extractor lives in the registry validator and only warns), str-2fjn (option a: one conformance case per command).

Size: M. Priority: P2. Type: task. Labels: parity, protocol, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 02-conformance-harness-correctness.md -->

---
slug: conformance-harness-correctness
kind: new
title: "Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, analyze/execute success cases never cross-check"
priority: P2
type: bug
labels: [conformance, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, analyze/execute success cases never cross-check

## Problem

`protocol/conformance/conformance_harness.py` under-reports and mis-reports in four ways:

1. After a timeout it keeps using the same frontend process, so a late reply is read as the answer to the next case.
2. `known_drifts` patterns are regexes but are tested as substrings, so they never match.
3. The summary line prints `frontends × cases = <executed checks>`, which is not a product.
4. Every analyze/execute success case runs on a single frontend, so the structural cross-check never sees `side_effects` or conditions, which are the fields the drifts exist for.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `conformance_harness.py:88-107` `send()` writes the request, calls `select` once with `COMMAND_TIMEOUT_S` (`:31`, 30 s), reads a single line, and does no id matching. When `resp is None`, the case loop (`:609-615`) records the failure and `continue`s on the same process. A drift-patrol run at load average 66/103/116 reported 4 Go timeouts and then `go / shutdown -- id: expected 99, got 20`. Reruns passed (`conformance_harness.py -f go` passed in 2.3 s; the full harness passed 42 checks).
- `conformance_harness.py:675`: `if any(pat in d for pat in known_drift_patterns):`. The patterns in `conformance_cases.yaml:16-24` are `side_effects.*thrown_error`, `side_effects.*global_mutation` and `condition.*ite`. On a sample drift string, the substring test gives [False, False, False] and `re.search` gives [True, False, False]. The first entry cites `side-effect-thrown-error-placement`, which does not exist in `parity-matrix.yaml`.
- `conformance_harness.py:682`: `print(f"Tested {n_frontends} frontends x {len(cases)} cases = {total_checks} checks")`. The drift-patrol log printed `Tested 4 frontends x 18 cases = 42 checks` (4×18 = 72) and contained 8 lines of `cross-check: SKIP only 1 frontend responded`.
- Every analyze/execute success case in `conformance_cases.yaml` (roughly :300-563, e.g. `execute_outcome_shape_go/ts/rust`) lists a single frontend. Only error, handshake, setup, teardown, generate and shutdown cases run on more than one frontend.
- Audit findings prior-05 (confirmed, P2), protocol-parity-02 (confirmed, P1 → P2), gates-10 (confirmed, P3; folded in here).

## Acceptance criteria

- [ ] `send()` reads lines until one arrives whose `id` equals the request id, discarding and logging stale lines. On a timeout, the frontend is killed, respawned and re-handshaken before the next case. One slow reply produces exactly one failure.
- [ ] known_drifts matching uses `re.search`. Every entry carries a `divergence_id` that `validate-parity.py` resolves against `allowed_divergences`. A run reports any entry that matched nothing. Alternative: delete known_drifts entirely and point GOVERNANCE, PARITY.md and the frontend-parity skill at `allowed_divergences`. The close note must say which option was chosen, because governance-md-omits-matrix and parity-guidance-skill-and-template follow it.
- [ ] The summary prints executed, skipped and cross-checked counts separately. No multiplication.
- [ ] At least one analyze success case and one execute success case run on every language frontend against equivalent fixtures, so the structural comparison covers the execute response (`side_effects`, `path_constraints`/conditions).
- [ ] Harness unit tests cover the id-mismatch/stale-line path, the respawn-after-timeout path and the regex-drift path, including an unmatched-drift report. Proof at close: each new test fails against the pre-fix harness and passes after the fix (paste both runs).
- [ ] `task conformance` passes after being forced to execute (not checksum-cached).

## Suggested approach

Fix `send()` and respawn first; this is what makes drift-patrol runs go red under load. Then fix drift matching (or delete it), then add the cross-frontend cases. Consider scaling `COMMAND_TIMEOUT_S` with a load-aware factor, as `run-heavy` does.

## Out of scope

- The Rust prepare timeout itself (str-qe9pp).
- Per-command dispatch coverage cases (parity-dispatch-reconciliation).
- Validating responses against JSON schemas (protocol-schemas-reject-real-output).

## Dependencies

- Blocked by: none.
- Related: str-qe9pp (open; Rust prepare timeout that triggers the id cascade), str-uoclg (closed; same cascade seen as a symptom), str-qwua7.34 (divergence-ID resolution), parity-guidance-skill-and-template, governance-md-omits-matrix.

Size: M. Priority: P2. Type: bug. Labels: conformance, protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 03-capability-single-source.md -->

---
slug: capability-single-source
kind: new
title: "Protocol capability facts are hand-copied in six places, and four parity-matrix sections are checked by no script"
priority: P2
type: task
labels: [parity, protocol, architecture, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Protocol capability facts are hand-copied in six places, and four parity-matrix sections are checked by no script

## Problem

Which commands and capabilities each frontend supports is maintained by hand in six places:

1. the frontend handshake arrays
2. the `frontends:` block of `protocol/registry.yaml`
3. `protocol/parity-matrix.yaml`
4. the golden handshake files
5. `protocol/PARITY.md`
6. the crate `CLAUDE.md` files and the frontend-parity skill

Only pairwise checks exist, so a fact can be correct in one copy and wrong in another (see protocol-parity-md-stale for live examples). Four matrix sections, `shared_wire_types`, `side_effect_capabilities`, `feature_capabilities` and `adapter_capabilities`, are read by no script. Agents still treat them as enforced; the frontend-parity skill calls the matrix "authoritative".

## Evidence (re-verified 2026-09-23 at 56c86168)

- The copies: TS `SUPPORTED_CAPABILITIES` in `shatter-ts/src/handlers.ts`; Go `CommandCapabilities` and `handleHandshake` in `shatter-go/protocol/handler.go`; Rust `handle_handshake` in `shatter-rust/src/handler.rs`; `protocol/registry.yaml:407` (`frontends:`); `protocol/parity-matrix.yaml:102` (`commands:`) and `:203` (`complex_type_capabilities:`); `protocol/conformance/golden/handshake/{typescript,go,rust,noop}.json`; the `protocol/PARITY.md` tables; `shatter-{ts,go,rust}/CLAUDE.md`; `.claude/skills/frontend-parity/SKILL.md`.
- The unvalidated sections are at `parity-matrix.yaml:18` (`shared_wire_types`), `:482` (`side_effect_capabilities`), `:589` (`feature_capabilities`) and `:881` (`adapter_capabilities`). `git grep -lE 'side_effect_capabilities|feature_capabilities|adapter_capabilities|shared_wire_types' -- . ':!audits' ':!.beads'` returns only `parity-matrix.yaml`, `conformance_cases.yaml` (comments), the skill, three crate `CLAUDE.md` files and a prose mention in `shatter-cli/src/commands/explore.rs`. No file under `scripts/` reads them.
- Audit finding protocol-parity-08 (confirmed, P2).

## Acceptance criteria

- [ ] `protocol/parity-matrix.yaml` is the single source of per-frontend capability status. The registry `frontends:` block and the golden handshake expectations are generated from it, or checked against it by a generator `--check` that runs in `task parity`. Handshake arrays in frontend source stay hand-written but are checked against the matrix. The existing detectors already do that; keep them.
- [ ] Each of the four unvalidated sections either gets a detector that `task parity` runs (for example a source grep for each side-effect emitter, or a conformance execute case that asserts presence), or gets a header comment and a matrix-level `enforced: false` marker that `validate-parity.py` reads and prints, so readers can tell documentation-only sections from enforced ones.
- [ ] Proof at close: a canary edit (flip one Rust complex-type capability in the matrix only) makes `task parity` fail when forced to execute. Paste the output. Then revert.
- [ ] The frontend-parity skill and crate `CLAUDE.md` tables are generated or pointer-only (coordinate with str-qwua7.24; do not duplicate its generator).

## Suggested approach

Extend the str-qwua7.24 table generator rather than writing a second one. Its `--check` mode should also cover the registry `frontends:` block and the golden handshake expectations. protocol-parity-md-stale can reuse the same generator for PARITY.md.

## Out of scope

- Generating the 13 registry enums for TS, Go and Rust (split out to protocol-codegen-all-registry-enums, because this issue is already L-sized).
- Dispatch-vs-advertisement reconciliation (parity-dispatch-reconciliation).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.24 (generated doc tables), str-qwua7.21.3, str-2fjn, protocol-parity-md-stale, protocol-codegen-all-registry-enums, protocol-md-execute-fields.

Size: L. Priority: P2. Type: task. Labels: parity, protocol, architecture, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 04-protocol-codegen-all-registry-enums.md -->

---
slug: protocol-codegen-all-registry-enums
kind: new
title: "protocol-codegen emits 6 of the 13 registry enums; error_category already differs between TS and the registry"
priority: P3
type: task
labels: [protocol, codegen, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# protocol-codegen emits 6 of the 13 registry enums; error_category already differs between TS and the registry

Split out of capability-single-source (manifest instruction: split codegen for the 13 registry enums when that issue exceeds about 2 days; it is sized L).

## Problem

`protocol/registry.yaml` declares 13 enums under `enums:`. `scripts/protocol-codegen.py` generates only the legacy vocabulary: commands, statuses, error codes, setup levels, generator kinds and branch types. The other enums are hand-copied in each language with no check, and one has already drifted.

## Evidence (re-verified 2026-09-23 at 56c86168)

- Registry enums (`python3 -c "import yaml; print(list(yaml.safe_load(open('protocol/registry.yaml'))['enums']))"`): setup_level, generator_kind, branch_type, outcome_status, value_plan_kind, value_requirement_kind, runtime_requirement_kind, apply_policy, error_category, unsatisfied_requirement_kind, discovered_dependency_kind, trace_event_type, crypto_boundary_kind.
- `shatter-ts/src/generated/protocol-enums.ts` exports only PROTOCOL_VERSION and ALL_COMMANDS, ALL_RESPONSE_STATUSES, ALL_ERROR_CODES, ALL_SETUP_LEVELS, ALL_GENERATOR_KINDS and ALL_BRANCH_TYPES (lines 11-89). The Go output (`shatter-go/protocol/protocol_enums_gen.go`) and the Rust FE parity test (`shatter-rust/tests/codegen_parity.rs`) cover the same subset.
- Drift: the registry has `error_category: [validation, runtime, infrastructure]`, but `shatter-ts/src/protocol.ts:631-635` `ErrorCategory` adds `"unknown"`. Core carries `error_category` as a `String` (`shatter-core/src/protocol.rs`, e.g. :1821).
- Audit finding protocol-parity-19 (confirmed, P3).

## Acceptance criteria

- [ ] `protocol-codegen.py` emits every `enums:` entry for TS, Go and the Rust frontend. `protocol-codegen.py --check` (already run in `task parity`) fails on drift for all 13.
- [ ] A shatter-core test asserts that the serde spelling of every core enum that mirrors a registry enum equals the registry values.
- [ ] The error_category mismatch is resolved: either the registry gains `unknown`, or TS drops it. The choice is recorded in the registry comment.
- [ ] Proof at close: add a value to one non-legacy registry enum without regenerating, and show `task parity` (forced to execute) failing. Paste the output, then revert.

## Suggested approach

Make the enum list data-driven: iterate `enums:` instead of naming the legacy six. Replace the hand-written TS/Go/Rust definitions of the new enums with imports of the generated ones.

## Out of scope

Capability single-sourcing (capability-single-source) and field-level codegen from `field_model`.

## Dependencies

- Blocked by: none.
- Related: capability-single-source, str-2fjn (its notes record the error_category mismatch).

Size: M. Priority: P3. Type: task. Labels: protocol, codegen, parity, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 05-protocol-schemas-reject-real-output.md -->

---
slug: protocol-schemas-reject-real-output
kind: new
title: "Published protocol JSON schemas reject real frontend output (shl/shr/bit_clear, complex constants), and no gate validates live output"
priority: P2
type: bug
labels: [protocol, schema, conformance, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Published protocol JSON schemas reject real frontend output (shl/shr/bit_clear, complex constants), and no gate validates live output

## Problem

`protocol/schemas/*.schema.json` is maintained by hand and validated only against hand-written fixtures. The core types have grown past the schemas, so real frontend responses fail them. External consumers and future frontend authors who trust the published schemas get a wrong contract.

## Evidence (re-verified 2026-09-23 at 56c86168)

- The Go analyzer emits `"shl"` for `<<` (`shatter-go/protocol/analyzer.go:2440`). During the audit, a Go analyze of `if x<<2 > 8` was validated against `protocol/schemas/response.schema.json` using the schemas' own resolver. It failed with `'shl' is not one of ['eq', …, 'instance_of']`. The verifier confirmed this from code but did not re-run the live validation.
- Core `BinOpKind` has `Shl`, `Shr` and `BitClear` (`shatter-core/src/sym_expr.rs:106-110`), and `ConstValue::Complex` exists (`sym_expr.rs:74-79`). `/usr/bin/grep -cE '"shl"|"shr"|"bit_clear"|complex' protocol/schemas/sym-expr.schema.json` → `0`.
- PROTOCOL.md's Binary Operators list (`PROTOCOL.md:546-548`) ends at `instance_of`, with no shl, shr or bit_clear.
- `protocol/schemas/test_schema_validation.py` validates checked-in fixtures only. `/usr/bin/grep -c schema protocol/conformance/conformance_harness.py` → `0`, so the harness never loads a schema.
- `protocol/GOVERNANCE.md:35` ("2. Update JSON schemas") is a manual step with no check behind it.
- Audit finding protocol-parity-03 (confirmed, P2).

## Acceptance criteria

- [ ] `sym-expr.schema.json` (and any schema that embeds its enums) accepts `shl`, `shr`, `bit_clear` and complex constants, and `PROTOCOL.md`'s operator and constant lists name them.
- [ ] One of the following, recorded in the close note:
  - a shatter-core test serializes one fully populated instance of every `SymExpr`, `BinOpKind` and `ConstValue` variant and validates each against the schema;
  - or the schemas are generated from core types (`schemars`) with a `--check` mode in `task schemas`.
- [ ] The conformance harness validates every response it receives against `response.schema.json` and fails on a violation.
- [ ] GOVERNANCE step 2 links to the new check (coordinate with governance-md-omits-matrix if that rewrite lands first).
- [ ] Proof at close: the new core test, or the harness validation, fails on the pre-fix schema (paste the output) and passes after the fix. `task schemas` and `task conformance` pass when forced to execute.

## Suggested approach

Start with the core round-trip-against-schema test. It is cheap and catches future enum additions. Add harness validation next. Generating the schemas with `schemars` is the long-term option, but it may change schema layout that external readers depend on, so decide that separately.

## Out of scope

Output-artifact schemas (spec, scan report and so on; shatter-docs bucket artifact-json-schemas).

## Dependencies

- Blocked by: none.
- Related: str-2fjn (option b compares registry field lists, not live schema validity), str-fpgb.5 (closed; fixtures only), str-a4c (closed; added shl/shr/bit_clear to core but not to the schema), conformance-harness-correctness, governance-md-omits-matrix.

Size: M. Priority: P2. Type: bug. Labels: protocol, schema, conformance, docs, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 06-protocol-parity-md-stale.md -->

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

---

<!-- file: 07-governance-md-omits-matrix.md -->

---
slug: governance-md-omits-matrix
kind: new
title: "protocol/GOVERNANCE.md, the required protocol-change checklist, omits the parity matrix, codegen and validate-parity, and names two authorities"
priority: P2
type: task
labels: [protocol, parity, docs, governance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# protocol/GOVERNANCE.md, the required protocol-change checklist, omits the parity matrix, codegen and validate-parity, and names two authorities

## Problem

Root `CLAUDE.md` and the frontend-parity skill send agents to `protocol/GOVERNANCE.md` as the checklist to follow for any protocol change. The file was last changed on 2026-05-05 (84a44654). Since then, the protocol-change machinery has grown to include codegen (`scripts/protocol-codegen.py`), `scripts/validate-parity.py`, the `parity-matrix.yaml` capability and `allowed_divergences` sections, and the generated TS/Go enum files. GOVERNANCE mentions none of them.

Following GOVERNANCE step by step therefore leaves the matrix, the generated enums and the divergence metadata out of date. The gaps show up later as `task parity` failures or as silent drift where no gate looks.

The file also names two sources of truth, the registry and core `protocol.rs`. It tells agents to record drift in `known_drifts`, which cannot match anything (see conformance-harness-correctness). And it describes a "source-name parity layer" for every frontend that is silently empty for TS (see validator-ts-extraction-empty).

## Evidence (re-verified 2026-09-23 at 56c86168)

- `/usr/bin/grep -cE 'parity-matrix|validate-parity|protocol-codegen|generated|PARITY.md|allowed_divergences' protocol/GOVERNANCE.md` → `0`.
- Two authorities: `GOVERNANCE.md:7` says "`protocol/registry.yaml` is the **single source of truth**". `:55-57` ("4. Implement in shatter-core (authoritative)") says core `protocol.rs` "is the authoritative implementation — frontends must match it".
- Step 1 (`:31`) never says to regenerate bindings (`python3 scripts/protocol-codegen.py --write`).
- The step 5 table (`:61-71`) points TS at `shatter-ts/src/protocol.ts`, but the vocabulary now lives in `shatter-ts/src/generated/protocol-enums.ts` and dispatch lives in `handlers.ts`. It points Rust at `shatter-rust/src/protocol.rs` only, but dispatch is in `shatter-rust/src/handler.rs`.
- `:78`, `:185`, `:209` and `:211-215` tell agents to record accepted differences in `known_drifts`. `allowed_divergences` in the matrix is never mentioned.
- `:96` says "Run **all five** checks". The list covers registry, schema, conformance, golden and per-language checks, and omits `protocol-codegen.py --check` and `validate-parity.py`. Both run in `task parity` (`Taskfile.yml:265-274`).
- `:113-114` "Source-name parity layer. Cross-checks command, response status, and error code names against source files in core and every frontend." TS extraction returns empty sets and is skipped (`scripts/validate-protocol-registry.py:545-553`, `:647`).
- CI Integration (`:173-181`) names `task schemas`, `task conformance` and `task golden-test`. It does not mention `task parity`, or the `task check` gate that CI actually runs.
- Audit finding protocol-parity-07 (confirmed, P2). Report section 15.1 lists it as a new issue with no prior draft.

## Acceptance criteria

- [ ] GOVERNANCE.md names exactly one authority for vocabulary and field model: the registry. It states that core serde must match the registry and names the test that enforces this. No other sentence calls a different file authoritative.
- [ ] The required steps form one ordered checklist, and every step a real protocol change needs is on it:
  1. registry
  2. `python3 scripts/protocol-codegen.py --write`
  3. core serde types
  4. each frontend, with its files listed per frontend, including dispatch files
  5. schemas and fixtures
  6. `parity-matrix.yaml` status, plus an `allowed_divergences` entry for any intended gap
  7. PARITY.md / PROTOCOL.md, per whatever protocol-parity-md-stale and protocol-md-execute-fields decide
  8. `task parity`, `task conformance` and `task schemas`
- [ ] The Validation Checks section lists every check that `task parity`, `task conformance` and `task schemas` actually run. A test (e.g. in `scripts/`) parses the command lists of those Taskfile tasks and fails if GOVERNANCE omits one. That is the doc-to-Taskfile consistency check that was missing.
- [ ] known_drifts guidance matches the outcome of conformance-harness-correctness (kept with `divergence_id`, or deleted). Either way, `allowed_divergences` is named as the registry of intended divergences.
- [ ] The source-name parity layer description matches what validator-ts-extraction-empty decides. If that issue is still open, describe the layer as it actually behaves and link the issue.
- [ ] Proof at close: the new consistency test fails against the pre-rewrite GOVERNANCE.md (paste the output) and passes after the rewrite.

## Suggested approach

Rewrite it as a short numbered checklist, and move the explanation into linked sections. Keep the per-frontend file table, but generate or `--check` it if capability-single-source adds a generator.

## Out of scope

Changing the validators themselves. Their fixes live in parity-dispatch-reconciliation, validator-ts-extraction-empty and conformance-harness-correctness. This issue documents what they do.

## Dependencies

- Blocked by: none. It can land first and describe current behavior. If conformance-harness-correctness or validator-ts-extraction-empty land first, their close notes decide the known_drifts and source-name wording.
- Related: str-2fjn and str-qwua7.7 (each asked for a one-sentence GOVERNANCE update), str-fpgb.9 (closed; created the original doc), conformance-harness-correctness, validator-ts-extraction-empty, protocol-schemas-reject-real-output, parity-guidance-skill-and-template.

Size: S. Priority: P2. Type: task. Labels: protocol, parity, docs, governance, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 08-protocol-md-execute-fields.md -->

---
slug: protocol-md-execute-fields
kind: new
title: "PROTOCOL.md's execute section documents about half of the request and response fields"
priority: P2
type: task
labels: [protocol, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# PROTOCOL.md's execute section documents about half of the request and response fields

## Problem

`PROTOCOL.md` is the published wire spec for frontend authors. Its `execute` section documents 4 of the 8 request fields and 8 of the 15 response fields that core and the registry define. Fields have been added by hand under GOVERNANCE step 8 ("update PROTOCOL.md if wire format changes"), and no check catches an omission. A new frontend written from PROTOCOL.md would not know about `plan`, `outcome`, `scope_events`, `loop_body_states` and the others.

## Evidence (re-verified 2026-09-23 at 56c86168)

- The registry `commands.execute.field_model` (`protocol/registry.yaml:161-190`) has these fields:
  - request: function, inputs, mocks, setup_context, prepare_id, capture, execution_profile, plan
  - response: return_value, thrown_error, branch_path, lines_executed, calls_to_external, path_constraints, scope_events, loop_body_states, side_effects, capture_truncation, performance, discovered_dependencies, connection_failures, outcome, runtime_crypto_boundaries
- Core carries the same fields: `Command::Execute` at `shatter-core/src/protocol.rs:410`, and `ExecuteResult` at `:1130-1186`.
- `PROTOCOL.md:209-334` (execute section) documents these fields:
  - request: function, inputs, mocks, capture
  - response: return_value, thrown_error, branch_path, lines_executed, calls_to_external, path_constraints, side_effects, performance
- These fields appear 0 times anywhere in PROTOCOL.md: `execution_profile`, `plan`, `scope_events`, `loop_body_states`, `capture_truncation`, `discovered_dependencies`, `connection_failures`, `runtime_crypto_boundaries`, `outcome`. `setup_context` and `prepare_id` appear only in the prepare and setup sections (`:168-203`, `:338-362`), not in execute.
- Audit finding protocol-parity-09 (confirmed, P2; the verifier noted the setup_context/prepare_id nuance above). Report section 15.1 lists it as a new issue with no prior draft.

## Acceptance criteria

- [ ] Every request and response field of every command in `registry.yaml` `field_model` appears in PROTOCOL.md with its type, whether it is optional, and a one-line description. Execute is the known-bad case, but the check covers all commands.
- [ ] Preferred: the per-command field tables are rendered from `field_model` into marked regions of PROTOCOL.md by a generator with a `--check` mode that runs in `task parity`. If capability-single-source (or str-qwua7.24) has landed a generator by then, extend it. Otherwise add a small renderer; do not wait. If generation is rejected, a test fails when a `field_model` field name is missing from the command's PROTOCOL.md section.
- [ ] `field_model` entries gain a `description` where one is missing, so the generated table is useful.
- [ ] The narrative JSON examples stay.
- [ ] Proof at close: add a dummy field to `field_model` without touching PROTOCOL.md, run `task parity` forced to execute, and show it failing (paste the output). Then revert.

## Suggested approach

Render a table below each command's examples, between `<!-- generated:field_model:<command> -->` markers. Have `protocol-codegen.py` own the rendering, since it already reads the registry.

## Out of scope

Operator and constant lists (protocol-schemas-reject-real-output) and error-code tables (done in str-iqta).

## Dependencies

- Blocked by: none. If capability-single-source lands first, reuse its generator.
- Related: str-iqta (closed; completed command, error-code and type tables but not per-command fields), str-2fjn, capability-single-source, governance-md-omits-matrix.

Size: S-M. Priority: P2. Type: task. Labels: protocol, docs, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 09-divergence-tracking-issue-liveness.md -->

---
slug: divergence-tracking-issue-liveness
kind: new
title: "Parity divergences marked `tracked` point at closed or deferred issues; add a tracking-issue liveness check"
priority: P2
type: task
labels: [parity, protocol, drift, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Parity divergences marked `tracked` point at closed or deferred issues; add a tracking-issue liveness check

## Problem

Entries in `protocol/parity-matrix.yaml` `allowed_divergences` with `status: tracked` are supposed to have a live owning issue. Three of them point at issues that are closed or deferred, so no one owns those divergences. Closing an implementation issue never prompts anyone to repoint the divergences that cite it. The validator checks only that `tracking_issue` is a non-empty string.

## Evidence (re-verified 2026-09-23 at 56c86168; tracker state via `bd show`)

- `go-symbolic-http-request-body` (`parity-matrix.yaml:1001`) is `tracking_issue: str-e41w`, and `bd show str-e41w` → CLOSED. Its `affected_frontends` are `[typescript, rust]`: the Go work is done and the gap is in TS/Rust, so the `go-` prefix is misleading. The ID is also referenced in `protocol/PARITY.md` and `shatter-go/CLAUDE.md`.
- `ite-symexpr-production-partial` (`:1096`, `affected_frontends: [rust]`) is `tracking_issue: str-1hlk.17`, which is CLOSED (it was the Go ite epic). No open issue covers Rust ite production.
- `ts-rust-execute-plan-not-implemented` (`:937`) is `tracking_issue: str-1hlk.16`, which is DEFERRED.
- `scripts/validate-parity.py:515-521` checks only that `tracking_issue` is a non-empty string or `none`.
- `scripts/drift-patrol.py:185-215` already loads tracker state: `bd list --all --json`, falling back to `.beads/issues.jsonl` when `bd` is absent (CI).
- Audit finding protocol-parity-12 (confirmed, P2).

## Acceptance criteria

- [ ] A drift-patrol check (it has tracker access; `validate-parity.py` stays tracker-free) FAILs when a `tracked` divergence's `tracking_issue` is closed or missing, and WARNs when it is deferred. Unit tests cover open, closed, missing and deferred using a fake tracker.
- [ ] When the only tracker source is the stale `.beads/issues.jsonl` export, the check reports SKIP with a reason instead of judging liveness from it. This is consistent with D4; see beads-jsonl-consumers-drop-bd-sync.
- [ ] For the two closed-issue entries (Rust ite production; TS/Rust symbolic request-body synthesis), successor issues are filed and the entries repointed, or the entries are reclassified `accepted` with a reason. str-1hlk.16 is either un-deferred or the entry gets a WARN-acknowledged reason.
- [ ] The misnamed ID is renamed (e.g. `native-request-body-synthesis-go-only`), and every reference is updated (matrix, PARITY.md, shatter-go/CLAUDE.md; find them with `git grep go-symbolic-http-request-body -- . ':!audits' ':!.beads'`).
- [ ] Proof at close: the new check run against the current matrix (before repointing) reports the two closed issues as FAIL (paste the output). `task parity` and `task drift-patrol` pass afterwards.

## Suggested approach

Add a `divergence-tracking-liveness` check to drift-patrol that reuses its existing Tracker loader. Keep `validate-parity.py` offline and deterministic.

## Out of scope

Implementing the divergent features (Rust ite, TS/Rust request-body synthesis, TS/Rust InvocationPlan).

## Dependencies

- Blocked by: none.
- Related: str-1hlk.12 (closed; requires a tracking issue but not a live one), str-qwua7.34 (divergence-ID resolution), beads-jsonl-consumers-drop-bd-sync (D4; decides what CI reads instead of the JSONL), drift-patrol-workflow-go-mod.

Size: S. Priority: P2. Type: task. Labels: parity, protocol, drift, agents, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 10-validator-ts-extraction-empty.md -->

---
slug: validator-ts-extraction-empty
kind: new
title: "validate-protocol-registry: TS source extraction is silently empty, which str-qwua7.7 was closed without fixing"
priority: P2
type: bug
labels: [protocol, parity, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [validator-optional-command-warning]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# validate-protocol-registry: TS source extraction is silently empty, which str-qwua7.7 was closed without fixing

## Problem

str-qwua7.7 ("validate-protocol-registry.py: empty source extraction must fail; point extractors at real sources") was closed at 0655458b. Its merged fix (4cf2165f) repointed only the Rust command extractor. The TS extractor still returns empty sets for commands, statuses and error codes, and `validate()` silently skips empty sets. The script therefore reports success while checking nothing for TS. Its title criterion, that empty extraction must fail, is unmet.

The 4cf2165f commit message says the vocabulary cross-check "is no longer this script's job". GOVERNANCE, by contrast, still describes a source-name parity layer for every frontend, so the code, the closed issue and the governance doc disagree about what the script is for.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `scripts/validate-protocol-registry.py:545-553` `extract_ts` reads `type Command = …`, `type ResponseStatus = …` and `type ErrorCode = …` unions from `shatter-ts/src/protocol.ts`. Those unions no longer exist there: `/usr/bin/grep -n "type Command\|type ResponseStatus\|type ErrorCode" shatter-ts/src/protocol.ts` finds nothing, because they moved to `shatter-ts/src/generated/protocol-enums.ts:26,42,59`. `extract_ts_union` returns `set()` when there is no match (:556-562).
- `validate()` (:641-669) does `if not src_set: continue` (:647), so the empty TS sets pass silently. The Rust `statuses` regex (:620-621) matches only some statuses. The audit dump found 5 of 11.
- `python3 scripts/validate-protocol-registry.py` → exit 0, `All checks passed (with informational warnings).`, with no TS line at all.
- `protocol/GOVERNANCE.md:113-114`: "Source-name parity layer. Cross-checks command, response status, and error code names against source files in core and every frontend."
- `bd show str-qwua7.7` → CLOSED.
- Audit finding protocol-parity-15 (confirmed, P2). The verifier corrected the dedupe: str-qwua7.7 is closed, not open, so this needs a new issue rather than a note on an open one.

## Acceptance criteria

- [ ] Decide and record in the close note:
  - (a) delete the TS layer and the statuses layer, because generated-enum `--check` plus the language sync tests cover vocabulary, and keep command extraction only where it reads real dispatch;
  - or (b) re-point TS extraction at the real sources: `handlers.ts` dispatch for commands, and the generated or handwritten status and error-code definitions.

  Either way, GOVERNANCE's description matches the result (see governance-md-omits-matrix).
- [ ] For every extractor that remains, an empty result for any category and any frontend is a hard error (non-zero exit) naming the frontend and the category.
- [ ] Canary test in `scripts/test_validate_protocol_registry.py`: point each remaining extractor at a fixture with the relevant definitions removed and assert a non-zero exit. The canary fails against today's script (paste the output) and passes after the fix.
- [ ] Rust status extraction either finds all 11 statuses or is removed under option (a).
- [ ] `task parity` passes when forced to execute; record the output.

## Suggested approach

Option (a) is probably right. Vocabulary is already guarded by `protocol-codegen.py --check`. "Advertised vs dispatched" belongs to parity-dispatch-reconciliation, which adds dispatch extractors to `validate-parity.py`. Share one dispatch-extractor helper so there are not two regex sets. Land after validator-optional-command-warning: re-pointing TS command extraction would otherwise add a new permanent TS `get_invocation_plan` warning, because the matrix marks it `not_implemented` for TS.

## Out of scope

Dispatch-vs-matrix reconciliation in `validate-parity.py` (parity-dispatch-reconciliation).

## Dependencies

- Blocked by: validator-optional-command-warning.
- Related: str-qwua7.7 (closed; see validator-reopen-note), parity-dispatch-reconciliation, governance-md-omits-matrix.

Size: S. Priority: P2. Type: bug. Labels: protocol, parity, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 11-validator-reopen-note.md -->

---
slug: validator-reopen-note
kind: reopen-note
title: "Note on closed str-qwua7.7: the empty-extraction criterion is unmet for TS"
priority: P2
type: note
labels: [protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.7
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-qwua7.7: the empty-extraction criterion is unmet for TS

Target: str-qwua7.7 (CLOSED at 0655458b). Action: add a comment. Do not reopen. The follow-up work is tracked in the new issue validator-ts-extraction-empty; the filer substitutes its str- id for the slug below.

## Comment text

Audit 2026-09-22 (finding protocol-parity-15, verified; re-checked 2026-09-23 at 56c86168): this issue was closed with its title criterion unmet.

- The fix (4cf2165f) repointed only the Rust command extractor at `handler.rs` dispatch. `extract_ts` (`scripts/validate-protocol-registry.py:545-553`) still reads `type Command/ResponseStatus/ErrorCode` unions from `shatter-ts/src/protocol.ts`. Those unions moved to `shatter-ts/src/generated/protocol-enums.ts`, so all three TS sets are empty.
- `validate()` skips empty sets (`if not src_set: continue`, :647). The script exits 0 with `All checks passed (with informational warnings).` and checks nothing for TS. "Empty source extraction must fail" is therefore not true.
- The Rust statuses regex (:621) finds only some of the 11 statuses.
- The 4cf2165f message says the vocabulary cross-check "is no longer this script's job", but `protocol/GOVERNANCE.md:113-114` still describes a source-name parity layer for every frontend. That decision was never reconciled with the issue or with GOVERNANCE.
- The Rust `get_invocation_plan` "may be unimplemented" warning still prints on every run, although the matrix marks it `not_implemented` for Rust.

Follow-ups: <validator-ts-extraction-empty> (decide delete-vs-repoint; hard-fail on empty extraction, with a canary test) and <validator-optional-command-warning> (the validator consults the matrix, so intended gaps do not warn).

---

<!-- file: 12-validator-optional-command-warning.md -->

---
slug: validator-optional-command-warning
kind: new
title: "validate-protocol-registry prints a permanent get_invocation_plan 'may be unimplemented' warning although the matrix marks it not_implemented for Rust"
priority: P3
type: chore
labels: [protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# validate-protocol-registry prints a permanent get_invocation_plan 'may be unimplemented' warning although the matrix marks it not_implemented for Rust

## Problem

Every run of the registry validator prints the same warning about an intended, documented gap. A warning that is always there teaches agents and humans to ignore validator warnings, including real ones.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `python3 scripts/validate-protocol-registry.py` prints:
  ```
  Warnings:
    shatter-rust/src/protocol.rs + handler.rs:
      commands: 'get_invocation_plan' in registry but not found in shatter-rust (may be unimplemented)

  All checks passed (with informational warnings).
  ```
- `protocol/parity-matrix.yaml:175-185`: `get_invocation_plan` has `status: optional` with `frontends: {typescript: not_implemented, go: implemented, rust: not_implemented}`.
- The warning is emitted in `validate()` at `scripts/validate-protocol-registry.py:663-666`. The validator never reads the matrix.
- Audit finding prior-24 (confirmed, P3).

## Acceptance criteria

- [ ] The validator reads `protocol/parity-matrix.yaml` `commands.<cmd>.frontends.<fe>` and suppresses the "may be unimplemented" warning when that frontend's status is `not_implemented` or `not_supported` for an optional command. A missing command whose matrix status is `implemented` stays reported, and should be a hard error; coordinate with parity-dispatch-reconciliation.
- [ ] A clean run on current main prints no warnings.
- [ ] Unit test in `scripts/test_validate_protocol_registry.py`: a command absent from source and marked `not_implemented` produces no warning, and one marked `implemented` does. Proof at close: the first test fails before the change and passes after it.
- [ ] `task parity` passes when forced to execute.

## Suggested approach

Load the matrix with the same YAML loader `validate-parity.py` uses, and pass per-frontend status into `validate()`.

## Out of scope

TS extraction (validator-ts-extraction-empty, which is blocked by this issue so that re-pointing TS does not add a TS `get_invocation_plan` warning).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.7 (closed), validator-ts-extraction-empty, parity-dispatch-reconciliation.

Size: S. Priority: P3. Type: chore. Labels: protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 13-protocol-rs-doc-comments.md -->

---
slug: protocol-rs-doc-comments
kind: new
title: "Core protocol.rs doc comments misstate who emits `outcome`, what `runtime_crypto_boundaries` does, and the error-code count"
priority: P3
type: bug
labels: [protocol, docs, core, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core protocol.rs doc comments misstate who emits `outcome`, what `runtime_crypto_boundaries` does, and the error-code count

## Problem

Three doc comments on the core wire types make false cross-frontend or behavioral claims. Readers and agents take doc comments on `shatter-core/src/protocol.rs` as the contract, so a wrong comment steers work. For example, it can lead someone to treat `outcome: None` from TS/Rust as normal, or to assume that crypto-boundary splitting exists.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `outcome`: `protocol.rs:1176-1184` says "TS / Rust frontends do not currently emit this field". Both do: TS `shatter-ts/src/executor.ts:3252` (`response.outcome = deriveOutcome(rawResult);`) and Rust `shatter-rust/src/handler.rs:1085` (`resp.outcome = Some(derive_execute_outcome(&result));`). The parity matrix says all three frontends support it.
- `runtime_crypto_boundaries`: `protocol.rs:1169-1173` says "The core engine uses this to apply boundary splitting: solve constraints on the plaintext then re-encrypt". The only consumers are `tracing::debug!` calls: `orchestrator.rs:3188-3191`, and `explorer.rs:1604-1610`, whose comment says it "will be used for boundary splitting in a future solver integration pass".
- `protocol.rs:2027`: "Canonical error code list (11 codes)" sits on `const ALL_ERROR_CODES: [(ErrorCode, &str); 12]` (:2030). The next lines claim "This match is exhaustive — adding a variant … causes a compiler error", but an array literal is not a match.
- Verifier correction (protocol-parity-10, partially confirmed, P2 → P3): the separate test `error_code_enum_is_exhaustive` (:2064-2086) already uses an exhaustive `match`, so a new `ErrorCode` variant does fail compilation. No new exhaustiveness mechanism is needed. Only the comments are wrong.

## Acceptance criteria

- [ ] The `outcome` comment says all three frontends emit it on execute responses, and that `None` means "not reported" (older or third-party frontends).
- [ ] The `runtime_crypto_boundaries` comment says the field is currently logged only and has no consumer in the solver.
- [ ] The `ALL_ERROR_CODES` comment gives the right count, or no count, and points to `error_code_enum_is_exhaustive` as the compile-time guard instead of claiming the array is one.
- [ ] Whether `runtime_crypto_boundaries` stays on the wire before it has a consumer is recorded: as a note on the relevant parity-matrix entry, or in a filed issue linked from the comment.
- [ ] `cargo test -p shatter-core --lib protocol` passes (comment-only change, no behavior change). The diff contains no code changes other than comments and the matrix note.

## Out of scope

Implementing crypto-boundary splitting, and changing the error-code tests.

## Dependencies

- Blocked by: none.
- Related: protocol-md-execute-fields (documents the same fields for frontend authors).

Size: XS. Priority: P3. Type: bug. Labels: protocol, docs, core, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 14-protocol-test-doubles-relocate.md -->

---
slug: protocol-test-doubles-relocate
kind: new
title: "Move protocol/ test-double frontends into a subdirectory and delete the dead delayed-execute-frontend.sh"
priority: P3
type: chore
labels: [protocol, cleanup, tests, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Move protocol/ test-double frontends into a subdirectory and delete the dead delayed-execute-frontend.sh

## Problem

`protocol/` holds the normative protocol documents: GOVERNANCE.md, PARITY.md, registry.yaml, parity-matrix.yaml and schemas/. Fifteen shell test-double frontends sit at the same level, which makes the directory hard to scan. One of them is referenced by nothing.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `ls protocol/*-frontend.sh | wc -l` → `15`: concolic-test, dead-after-handshake-once, delayed-execute, execute-exits-twice, failing-file, failing-generator, fixed-branch, generated-skip, id-mismatch, noop, observer-recording, slow-execute, slow, slow-once, unknown-branch-fuzz.
- `git grep -c delayed-execute -- . ':!audits' ':!.beads'` → `protocol/delayed-execute-frontend.sh:2`, which means only self-references. The file was last touched in 145e94a7 (2026-05-24).
- References to move: `git grep -F -- '-frontend.sh' -- . ':!audits' ':!.beads'` finds 62 matching lines across PROTOCOL.md, docs/plans/str-kapl-resilience-timeouts-memory.md, protocol/conformance/{conformance_cases,golden_cases}.yaml, shatter-cli/tests/scan_analysis_cache_identity.rs, shatter-core/src/{batch_analyze,explorer,frontend,orchestrator,scan_orchestrator}.rs, and the scripts themselves. Use `-F`: without it, `.` matches any character and pulls in unrelated prose.
- `PROTOCOL.md:602-608` documents `protocol/noop-frontend.sh` as the reference implementation for new frontend authors.
- Audit finding protocol-parity-20 (confirmed, P3).

## Acceptance criteria

- [ ] The test doubles live in `protocol/test-frontends/`. Every reference found by the grep above is updated, and the grep returns no `protocol/<name>-frontend.sh` paths outside the new directory.
- [ ] `noop-frontend.sh` either moves too, with PROTOCOL.md:602-608 updated, or stays in `protocol/` as the documented reference implementation. The close note records which.
- [ ] `protocol/delayed-execute-frontend.sh` is deleted.
- [ ] Taskfile `sources:` globs that cover `protocol/**/*` still include the moved files. Check `conformance` and any shatter-core test task.
- [ ] Proof at close: `task test-standard`, `task conformance` and `task e2e` pass when forced to execute (not checksum-cached); paste the gate lines.

## Out of scope

Rewriting the test doubles.

## Dependencies

- Blocked by: none.
- Related: none.

Size: S. Priority: P3. Type: chore. Labels: protocol, cleanup, tests, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 15-parity-guidance-skill-and-template.md -->

---
slug: parity-guidance-skill-and-template
kind: new
title: "Rewrite the frontend-parity skill workflow and extend the frontend-issue-template parity checklist to the Go and Rust builders"
priority: P3
type: task
labels: [agents, skills, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rewrite the frontend-parity skill workflow and extend the frontend-issue-template parity checklist to the Go and Rust builders

## Problem

The parity guidance that agents load is stale. The `frontend-parity` skill's hand-copied capability tables contradict `protocol/parity-matrix.yaml`. Its "When you're about to..." steps send agents to `known_drifts`, a mechanism that cannot currently match anything (see conformance-harness-correctness), and they never mention `allowed_divergences` or `validate-parity.py`. The parity checklist in `protocol/frontend-issue-template.md` names only the TS SymExpr builders and the core explorer/orchestrator pair. It leaves out the Go and Rust dual-builder pairs, which is where str-qwua7.35 and str-qwua7.36 found drift.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `.claude/skills/frontend-parity/SKILL.md` was last changed on 2026-05-05 (56ac9c81).
  - `:37` shows Go `thrown_error` as ✗; the matrix has `go: captured`. The audit also found Go `file_write`, `network_request` and `environment_read` marked ✗ in the same table while the matrix says captured.
  - `:59` says "TS is the only frontend that produces `ite` … Rust's analyze handler is a stub". Go produces ite (str-1hlk.17.3), and Rust analyze is implemented.
  - `:69` says the Rust timeout is "stored, not yet applied — execute unimpl".
  - `:23` and `:75` tell agents to "document the drift in `known_drifts`".
  - The file has no mention of `validate-parity.py` or the PARITY.md mirror rule. `allowed_divergences` appears only as a table description (`:22`), never as a workflow step.
- `protocol/frontend-issue-template.md` "Parity Impact" (around lines 11-19) lists TS `buildSymExpr`/`buildSymExprWithFlow`, `explorer.rs`/`orchestrator.rs` and `main.rs` CLI wiring only. It is missing:
  - Go `shatter-go/protocol/analyzer.go` vs `shatter-go/instrument/symextract.go`, plus the loop-snapshot builder in `shatter-go/protocol/loop_body_states.go`
  - Rust `shatter-rust/src/analyzer.rs` `build_sym_expr` (:1954) vs `shatter-rust/src/instrument.rs` `constraint_for_expr` (:519)
  - a "protocol-visible? → matrix entry or `allowed_divergences` entry, then `task parity` + `task conformance`" item
- Audit findings protocol-parity-17 (confirmed, P3) and protocol-parity-18 (confirmed, P3).

## Acceptance criteria

- [ ] The skill's capability tables are removed in favour of a pointer to the matrix, or generated by the str-qwua7.24 generator. No hand-maintained ✓/✗ table remains.
- [ ] The skill's workflow steps point at `allowed_divergences` in `parity-matrix.yaml`, `task parity` and `task conformance`. The `known_drifts` advice is removed, or kept only in the form conformance-harness-correctness chose (for example "known_drifts entries must carry a `divergence_id`").
- [ ] The false statements at `:59` and `:69` are corrected or removed.
- [ ] The issue template lists the Go and Rust builder pairs above and the matrix/divergence step.
- [ ] Proof at close: `/usr/bin/grep -n "✗\|stub\|not yet applied" .claude/skills/frontend-parity/SKILL.md` shows no stale capability claims (paste the output), and the template diff shows the new checklist items.

## Out of scope

Fixing the conformance harness known_drifts matching (conformance-harness-correctness), and the table generator itself (str-qwua7.24 / capability-single-source).

## Dependencies

- Blocked by: none. If conformance-harness-correctness is still open, word the known_drifts guidance as "do not use; register in allowed_divergences".
- Related: str-qwua7.24 (partially covers the tables), conformance-harness-correctness, governance-md-omits-matrix, protocol-parity-md-stale.

Size: S. Priority: P3. Type: task. Labels: agents, skills, parity, audit. Parent: Epic: Audit 2026-09-22 findings.

---

<!-- file: 16-qwua7-37-premise.md -->

---
slug: qwua7-37-premise
kind: note-to-existing
title: "Note for str-qwua7.37: the Step 0 premise is wrong for Go package functions, and the data path is wrong"
priority: P3
type: note
labels: [agents, protocol, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.37
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note for str-qwua7.37: the Step 0 premise is wrong for Go package functions, and the data path is wrong

Target: str-qwua7.37 (OPEN, P2: "Write a one-page SymExpr construction spec and record or fix the Go call-shape divergence"). Action: add a comment (`bd comments add`, or append to notes). Do not change status or priority.

## Comment text

Audit 2026-09-22 correction (finding protocol-parity-14; evidence in `audits/2026-09-22/areas/protocol-parity.md`; re-checked 2026-09-23 at 56c86168):

- Step 0 says Go's `name="recv.Method", receiver=null` call form "cannot be solved" and must become bare name plus receiver. That is wrong for package-qualified free functions:
  - `shatter-core/data/string-ops.yaml:34` declares `{ language: go, method: "strings.HasPrefix", style: free }`.
  - `shatter-core/src/solver.rs:919` has a "Go-style: no receiver, two positional args (e.g. strings.Contains(s, substr))" branch, with tests around :2420.
  - Live Go analyze output for `strings.HasPrefix(s, "go_")` is `{name: "strings.HasPrefix", args: [param s, const]}`, which is solvable today.
- The real divergence is method calls on values. `u.IsAdmin()` emits `{name: "u.IsAdmin", args: []}`, because `callSymExpr` (`shatter-go/protocol/analyzer.go:2359`) uses the whole selector as the name and drops the receiver. `collect_param_names` (`shatter-core/src/sym_expr.rs:125`) walks only receiver and args, so it loses param `u`.
- Proposed amended canonical form: package-qualified free functions keep `name="pkg.Fn", receiver=null` (matching string-ops.yaml `style: free`). Method calls on values emit `name=<bare method>, receiver=<expr>`. To tell them apart, the Go side must resolve whether the selector's X is a package identifier (`types.PkgName`) or a value.
- Add a regression test that Go `strings.*` constraints stay solvable after the change, and a test that a value-method call keeps its receiver param in `collect_param_names`.
- Path fix: the file is `shatter-core/data/string-ops.yaml`, not `data/string-ops.yaml`.
- The verifier rated this P3. The issue's acceptance text ("receiver = expr or null") already leaves room for the right answer, but the Step 0 wording would steer an implementer toward breaking free functions, so amend the body.
