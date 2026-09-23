# protocol/PARITY.md hand-mirrors the parity matrix and is stale; the validator compares only divergence IDs

- Priority: P2
- Type: bug
- Labels: parity,protocol,docs
- Tracker action: new issue (str-qwua7.24 generates the crate CLAUDE.md/skill tables and explicitly leaves protocol/PARITY.md as a pointer target; str-qwua7.45 covers the root PARITY.md only)
- Related: str-qwua7.24, str-qwua7.45
- Source findings: audit 2026-09-22 protocol-parity-06 (confirmed)

<!-- body -->
## Evidence / current code facts
- `protocol/PARITY.md:76-78` says Rust has "No complex type capabilities are implemented yet". The registry, matrix and handshake golden all list Rust `uuid, url, date, date_time`.
- The Go table omits `go_byte`, and the command table omits `get_invocation_plan`. Both are in the registry and matrix.
- The `adapter-owned-instrumentation-coverage-partial` block (around `:283`) says "Affected frontends: go, rust". The matrix has `[rust]` (Go was fixed by str-1qd5i).
- `:139` says "All 11 error codes". `protocol/registry.yaml` has 12.
- `scripts/validate-parity.py:419-445` (`parity_md_divergence_ids`) compares only heading IDs, not content.

## Acceptance criteria
- PARITY.md's capability tables and divergence blocks are generated from `protocol/parity-matrix.yaml`, with a `--check` mode run by `task parity`. Alternatively, the tables and mirror are deleted and PARITY.md links to the matrix.
- All four errors above are gone.
- If generation is chosen, reuse the generator from str-qwua7.24.
