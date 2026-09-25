---
slug: rust-input-deserialize-classification
kind: new
title: "Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors"
priority: P2
type: bug
labels: [rust-frontend, reporting, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors

All paths are relative to the shatter repo root (github.com/shatterproof-ai/shatter). Line numbers
were verified at 16794cef.

## Problem

Sometimes the Rust harness cannot deserialize an input into the parameter's type, for example
`input 1 deserialization failed: invalid value: integer `-1`, expected usize`. When that happens,
the target function never runs. Explore and scan output still record the row as `throws
runtime_error: ...`, which reads as a behaviour of the target.

Any input the generator gets wrong becomes a fake finding. Today the main source is the integer
bug `int-unsigned64-clamp`, but any future type-mapping gap would do the same. The walkthrough error
regex does not match "deserialization failed", so no gate notices.

This issue stands alone. It is related to the epic `int-width-signedness-epic` but is not a child:
it does not depend on the generator fix, and the epic does not depend on it. Its proof feeds bad
inputs directly, so it still works after the generator is fixed.

## Evidence

The generated harness hard-codes `"error_type": "runtime_error"` for deserialization failures at
three groups of sites in `shatter-rust/src/executor.rs`:

- the direct-call harness: `:2471`, `:2484`, `:2498`;
- the `'shatter_arm` dispatch: `:2792`, `:2805`, `:2819`;
- the JSON-literal harness: `:5005`, `:5016`, `:5030`.

Reproduction on 16794cef (recorded 2026-09-24) uses the fixture `examples/rust/int-width` as
drafted in `int-unsigned64-clamp`. If that issue has not landed yet, commit the fixture here; it is
given inline there. The command is:

```bash
cargo build -p shatter-cli && cargo build --manifest-path shatter-rust/Cargo.toml
tmp=$(mktemp -d) && cp -r examples/rust/int-width "$tmp"/
target/debug/shatter explore "$tmp/int-width/src/lib.rs:rank_usize" --allow-host-writes \
  --max-iterations 60 --request-timeout 240
```

It printed 15 paths, 13 of them `throws `runtime_error: input 1 deserialization failed: invalid
value: integer `-N`, expected usize``. This reproduction depends on the generator bug, so after
`int-unsigned64-clamp` lands, use the seeded tests below instead.

Go has the same class of problem, tracked as str-4yc9w (open, P1, started: the launcher's decode
errors bypass outcome classification). str-cfsa (closed) was an earlier Go counterpart. `bd search
deserializ` and `input_error` find no Rust-side duplicate.

## Acceptance criteria

- [ ] All nine sites above emit a distinct classification for a failed parameter decode: either a
  `thrown_error.error_type` such as `input_error`, or a distinct execute-result outcome. It must be
  the same one str-4yc9w chooses for Go. Coordinate on str-4yc9w before picking, and name the
  choice in the close note.
- [ ] The core and report layers treat that outcome as a tool or input error. It is not counted as
  a target behaviour or finding in explore and scan output, and it is counted in the run's error
  summary.
- [ ] Harness-level test in shatter-rust: build the harness for `rank_usize` from the fixture and
  execute it with the explicit input `["en", -1]`. Assert the new classification. This does not
  depend on the generator, so it keeps working after `int-unsigned64-clamp`. Show it failing on
  main.
- [ ] Report-level test: a raw result carrying the new classification is not rendered as `throws`
  and appears in the error summary. Show it failing on main.
- [ ] E2E in `shatter-core/tests/e2e_concolic_rust.rs`:
  - it uses `repo_examples_rust_dir().join("int-width/src/lib.rs")`, so no external checkout is
    needed;
  - it passes `vec![vec![json!("en"), json!(-1)]]` as the explicit seed to `orchestrator::explore`;
  - it asserts that the seed's result carries the new classification and is not a completed or
    `runtime_error` outcome.

  Run it with
  `cargo build --manifest-path shatter-rust/Cargo.toml && cargo test --test e2e_concolic_rust <name> -- --include-ignored`
  and paste the failing and passing `test result:` lines.
- [ ] If the classification is protocol-visible (a new `error_type` value or outcome), update
  `protocol/parity-matrix.yaml` and `shatter-rust/CLAUDE.md`, and run `task parity` and
  `task conformance`.

## Out of scope

- Fixing the generator (`int-unsigned64-clamp`).
- The Go-side change (str-4yc9w), beyond agreeing on the shared classification.

## Size

S

## References

- Split from `int-unsigned64-clamp` after the Codex cross-check of the 2026-09-22 audit (finding
  goals-15).
- Related: str-4yc9w (open, Go), str-cfsa (closed, Go), epic `int-width-signedness-epic` (related,
  not the parent).
