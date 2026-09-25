---
slug: go-uint-alias-removal
kind: new
title: "Remove the deprecated go_uint/go_byte TypeInfo aliases after the compatibility window (one continuous release at least 30 days old)"
priority: P3
type: task
labels: [go-frontend, protocol, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-int-width-sign-emission]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove the deprecated go_uint/go_byte TypeInfo aliases

A deferred follow-up to epic `int-width-signedness-epic`. It is not a child of that epic: it is
parented to the audit epic and does not hold the integer epic open. All paths are relative to the
shatter repo root (github.com/shatterproof-ai/shatter).

## Why

`core-go-alias-int-range` makes the core treat `go_uint`/`go_byte` as unsigned `Int` on the
`int_range` path, and `go-int-width-sign-emission` moves the Go analyzer to
`{"kind":"int","int_width","int_signed"}`. The core keeps accepting `go_uint`/`go_byte` as
deprecated wire aliases so that an older installed `shatter-go` binary still works with a newer
core. After the compatibility window, the aliases are
dead code and a second way of saying the same thing, which is the exact divergence the epic
removes.

## Trigger: do not start before this holds

Releases are the `continuous-<YYYYMMDD-HHMM>-<12-char sha>` prereleases created on every push to
`main` (`.github/workflows/release.yml`, the "Generate continuous build tag" step). The window has
passed when **at least one continuous release whose commit contains the merge of
`go-int-width-sign-emission` was created 30 or more days ago**. Retention keeps the 30 most recent
releases plus 12 monthly ones (`cleanup-continuous-releases.yml`), so such a release stays
listable. Verify it and paste the output:

```bash
merge=<sha of the go-int-width-sign-emission merge commit on main>
git fetch origin --tags
gh release list --repo shatterproof-ai/shatter --limit 500 --json tagName,createdAt \
  --jq '.[] | select(.tagName | startswith("continuous-")) | "\(.createdAt) \(.tagName)"' |
while read -r created tag; do
  sha=${tag##*-}
  if git merge-base --is-ancestor "$merge" "$sha" 2>/dev/null &&
     [ "$(date -d "$created" +%s)" -le "$(date -d '30 days ago' +%s)" ]; then
    echo "window passed: $tag ($created)"
  fi
done
```

At least one `window passed:` line is required. If there are none, leave the issue open and add a
comment with the date of the oldest qualifying release.

## Acceptance criteria

- [ ] The trigger output above is pasted.
- [ ] `ComplexKind::GoUint`/`GoByte` and their generators, mutators and serializer branches are
  removed from the core: `input_gen.rs` (`generate_go_uint`, `generate_go_byte`, `mutate_go_uint`,
  `mutate_go_byte`, dispatch arms), `orchestrator.rs` (`concrete_to_json` arms), `types.rs`,
  `export.rs`, `test_arbitraries.rs`, and the alias normalization added by
  `core-go-alias-int-range`. That issue's golden compatibility test is replaced by the rejection
  test below. Regenerate the bindings (`shatter-rust/src/protocol.rs`,
  `shatter-ts/src/protocol.ts`) from the schema; never hand-edit them. Remove the entries from
  `protocol/registry.yaml` and `protocol/parity-matrix.yaml`.
- [ ] `git grep -n 'go_uint\|go_byte\|GoUint\|GoByte' -- . ':!audits' ':!.beads' ':!docs/perf'`
  returns only the SPEC §8 changelog history and tests that assert rejection. Paste the output.
- [ ] Test: a TypeInfo payload with `complex_kind: "go_uint"` or `"go_byte"` now fails to
  deserialize, or maps to `Unknown` if the codebase's unknown-kind policy says so; state which,
  with a clear error. The test lives beside the existing `types.rs` ComplexKind serde tests.
- [ ] `task e2e-go` still passes, including `e2e_go_int_width_inputs_in_bounds` from
  `go-int-width-sign-emission`, and the Go bounds checker from that issue still prints
  `out of bounds: 0`.
- [ ] A SPEC §8 changelog row records the removal. `shatter-go/CLAUDE.md` is updated. `task parity`,
  `task conformance` and `task affected` pass, with `Gates selected` recorded.

## Out of scope

Any behaviour change for integer generation. This is a pure deletion.

## Size

S

## References

- `core-go-alias-int-range`: turns `go_uint`/`go_byte` into core-side aliases of unsigned `Int`.
- `go-int-width-sign-emission`: stops the Go frontend emitting them; its merge starts the window.
- str-cfsa (closed): `go_uint`.
- str-ieuc (closed): `go_byte`.
- No existing tracker issue covers the removal.
