---
slug: go-int-width-sign-emission
kind: new
title: "Go analyzer: emit int_width/int_signed for every integer kind instead of bare int and go_uint/go_byte; int8/int16/uint16 params currently get out-of-range values"
priority: P2
type: bug
labels: [go-frontend, protocol, parity, input-generation, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [core-go-alias-int-range]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go analyzer: emit int_width/int_signed for every integer kind

Step 4 of epic `int-width-signedness-epic`. Owner: `shatter-go` (plus the protocol registry,
parity matrix and conformance files its handshake change touches). The core side, which makes the
old `go_uint`/`go_byte` wire kinds behave as unsigned ints, is the prerequisite
`core-go-alias-int-range`. Removing the aliases from the core is the separate deferred follow-up
`go-uint-alias-removal`. All paths are relative to the shatter repo root
(github.com/shatterproof-ai/shatter). Line numbers were verified at 16794cef.

## Problem

The protocol's `kind: "int"` carries `int_width`/`int_signed`
(`protocol/schemas/type-info.schema.json:18-25`), and the Rust frontend fills them in. The Go
analyzer does not, and the Go `TypeInfo` struct (`shatter-go/protocol/types.go:329`) has no fields
for them.

**Signed kinds** (`int8`, `int16`, `int32`/`rune`, `int64`, `int`) map to a bare `{"kind":"int"}`
(`shatter-go/protocol/analyzer.go:1489`, `:1506` in `basicTypeInfo`; `:1561` in
`typeInfoFromAST`). The core treats them as full `i64`, so `int8` gets 128 and `i64::MIN`, which
`json.Unmarshal` rejects before the target runs.

**Unsigned kinds** use Go-only complex kinds instead of the protocol fields:

- `uint`, `uint16`, `uint32`, `uint64` and `uintptr` map to `go_uint` in `basicTypeInfo`
  (`:1502`); `typeInfoFromAST` maps `uint`..`uint64` (`:1565`) and has no `uintptr` case;
- `uint8`/`byte` maps to `go_byte` (`:1496`, `:1558`).

`go_uint` carries no width, so even after `core-go-alias-int-range` routes it onto
`Int { 64, false }`, a `uint16` or `uint32` param still gets values up to `u64::MAX`. Only the
analyzer knows the real width.

Go-side consumers of the current TypeInfo that must keep working:

- the planner: `shatter-go/planner/param.go:692-716` (`go_uint` → `uintFamily`; `[]byte`
  detection from a `go_byte` element, str-79nvf) and `planner/composite.go:213-244`
  (unsigned pointer and struct-field zero values);
- the handler: `protocol/handler.go:306` (declares `complex_type:go_byte`) and the direct-execute
  checks at `:2227` and `:2337`;
- `protocol/registry.yaml:454` (Go's declared complex types), `protocol/parity-matrix.yaml:458`
  (the `go_byte` row) and `protocol/conformance/golden/handshake/go.json:9`.

The core needs no change here: `core-go-alias-int-range` already treats both the old and the new
wire forms as unsigned ints, and requests from core to frontend carry no TypeInfo.

## Fixture (commit with this issue's branch)

`examples/go/int-width/go.mod`:

```
module example.com/int-width

go 1.23.0
```

`examples/go/int-width/widths.go`:

```go
// Package intwidth is the known-answer fixture for Go integer width and
// signedness. Each function branches at its parameter type's min or max.
package intwidth

func AtMaxInt8(n int8) string {
	if n == 127 {
		return "max"
	}
	return "other"
}

func AtMinInt8(n int8) string {
	if n == -128 {
		return "min"
	}
	return "other"
}

func AtMaxUint16(n uint16) string {
	if n == 65535 {
		return "max"
	}
	return "other"
}

func AtMaxUint64(n uint64) string {
	if n == 18446744073709551615 {
		return "max"
	}
	return "other"
}

func AtMaxByte(b byte) string {
	if b == 255 {
		return "max"
	}
	return "other"
}

func Label(tag string, n int16) string {
	if len(tag) > 2 {
		if n > 3 {
			return "big"
		}
		return "small"
	}
	return "none"
}
```

`scripts/check_go_int_width_bounds.py`:

```python
#!/usr/bin/env python3
"""Assert every executed integer input in the Go int-width fixture artifacts is within its type's bounds."""
import glob, json, sys

BOUNDS = {  # function -> (param index, min, max)
    "AtMaxInt8": (0, -128, 127), "AtMinInt8": (0, -128, 127),
    "AtMaxUint16": (0, 0, 65535), "AtMaxUint64": (0, 0, 2**64 - 1),
    "AtMaxByte": (0, 0, 255), "Label": (1, -32768, 32767),
}
bad = seen = 0
for f in sorted(glob.glob(sys.argv[1] + "/explore-results/*/*.json")):
    d = json.load(open(f))
    fn = d.get("function_name")
    if fn not in BOUNDS or "observation" not in d:
        continue
    i, lo, hi = BOUNDS[fn]
    for inputs, _mocks, _result in d["observation"]["raw_results"]:
        seen += 1
        n = inputs[i]
        if not (isinstance(n, int) and not isinstance(n, bool) and lo <= n <= hi):
            bad += 1
            print(f"OUT OF BOUNDS {fn}: {n!r}")
print(f"checked {seen} executed inputs; out of bounds: {bad}")
sys.exit(1 if bad or not seen else 0)
```


## Baseline (run on the branch base before changing code)

This needs no external examples checkout. Commit the fixture and checker first, then run from the
repo root:

```bash
git rev-parse HEAD
cargo build -p shatter-cli && (cd shatter-go && go build -buildvcs=false -o bin/shatter-go .)
export PATH="$PWD/shatter-go/bin:$PATH"
tmp=$(mktemp -d) && cp -r examples/go/int-width "$tmp"/
target/debug/shatter explore "$tmp/int-width/widths.go" --allow-host-writes \
  --max-iterations 60 --request-timeout 240
python3 scripts/check_go_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
```

Recorded at 16794cef on 2026-09-24, before any epic child landed: the checker exits 1 with
`checked 360 executed inputs; out of bounds: 80`. Examples:

- `AtMaxInt8`: 128, 671, -303, `i64::MIN`, `i64::MAX`;
- `AtMaxUint16`: 65536, 4294967295, 18446744073709551615, `null`;
- `AtMaxByte`: 256;
- `Label`'s `int16`: `i64::MAX`, `i64::MIN`;
- `AtMaxUint64`: `null`.

The branch base will already include `int-unsigned64-clamp`, `core-int-range-i128` and
`core-go-alias-int-range`, so the count will differ. It must still be non-zero: the signed kinds
are bare `int` and `uint16` is still a width-less `go_uint`. Paste the actual output. In the report
out-of-range values appear as `throws function_error: param n: json: cannot unmarshal number 128
into Go value of type int8`. The min/max branches are already reached, through Z3 on the literals
and the unsigned boundary seeds. That coverage must not regress.

## Acceptance criteria

- [ ] The fixture and checker are committed. The baseline output is pasted with
  `git rev-parse HEAD`.
- [ ] **Analyzer.** The Go `TypeInfo` gains `IntWidth`/`IntSigned` (`json:"int_width,omitempty"`,
  `json:"int_signed,omitempty"`). Both mapping sites (`basicTypeInfo` and `typeInfoFromAST`) emit
  `{"kind":"int","int_width":W,"int_signed":S}` for every Go integer kind, including `uintptr` in
  both. `int`, `uint` and `uintptr` use width 64, and a comment states the 64-bit-platform
  assumption. `rune` is `(32, true)`; `byte` is `(8, false)`. Neither site emits `go_uint` or
  `go_byte` any more. Analyzer unit tests cover every kind at both sites.
- [ ] **Planner and handler.** They recognize unsigned ints and `[]byte` from the new TypeInfo
  (`int_signed: false`; an array whose element is `int_width: 8, int_signed: false`) in
  `planner/param.go`, `planner/composite.go` and `handler.go:2227`, `:2337`. The existing
  str-79nvf and str-ieuc tests pass unchanged, and new cases cover the int-typed forms.
- [ ] **Handshake and registry.** The Go frontend stops declaring `complex_type:go_byte`
  (`handler.go:306`). Update `protocol/conformance/golden/handshake/go.json` and Go's entry in
  `protocol/registry.yaml` (`:454`) to match. The `go_byte` kind itself stays in the schema and in
  the registry's kind list until `go-uint-alias-removal`, so no bindings are regenerated here.
- [ ] **Parity.** `protocol/parity-matrix.yaml` gains an "integer width and signedness" row: Rust
  and Go emit it; TS is n/a (`number` is float and `bigint` is `big_int`). Update the `go_byte` row
  (`:458`) to say Go no longer emits it. Add a conformance case per emitting frontend that asserts
  the TypeInfo for the fixture's params. `task parity` and `task conformance` pass.
- [ ] **E2E** `e2e_go_int_width_inputs_in_bounds` in `shatter-core/tests/e2e_concolic_go.rs`,
  using `repo_examples_go_dir().join("int-width").join("widths.go")` and
  `spawn_go_frontend("int-width")`. For every fixture function it seeds with
  `boundary_dict::generate_boundary_inputs(&analysis.params)` and runs `orchestrator::explore`. It
  asserts:
  - every value in `raw_results` is within the Go type's bounds (the checker's table);
  - the `max`/`min` branch of each `At*` function is taken;
  - `Label` reaches `"big"`.

  It must not assert on error rows. Run it with
  `task go:build && SHATTER_GO_FRONTEND_BIN="$PWD/shatter-go/bin/shatter-go" cargo test --test e2e_concolic_go e2e_go_int_width -- --include-ignored`.
  The case is `#[ignore]`d, and no `SHATTER_EXAMPLES_DIR` is needed for a repo-local fixture.
  Paste the failing `test result:` line from the branch base and the passing line from the branch.
- [ ] Re-run the baseline on the branch, once with the default explorer and once with
  `--concolic`, from a fresh copy each time so artifacts from the first run cannot be resumed:
  ```bash
  for mode in "" "--concolic"; do
    tmp=$(mktemp -d) && cp -r examples/go/int-width "$tmp"/
    target/debug/shatter explore "$tmp/int-width/widths.go" --allow-host-writes \
      --max-iterations 60 --request-timeout 240 $mode
    python3 scripts/check_go_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
    rm -rf "$tmp"
  done
  ```
  Both checker runs print `out of bounds: 0`.
- [ ] A `SPEC.md` §8 changelog row marks `go_uint`/`go_byte` deprecated: no frontend emits them,
  the core still accepts them (`core-go-alias-int-range`) so an older installed `shatter-go` keeps
  working, and removal is `go-uint-alias-removal`.
- [ ] `shatter-go/CLAUDE.md` documents the protocol-visible change. `task e2e` and
  `task affected` pass, with `Gates selected` recorded.
- [ ] The close reason links `go-uint-alias-removal` (filed with the audit drafts) and gives the
  merge commit sha that starts its compatibility window. This issue does not wait for any release.

## Out of scope

- Any core change (`core-go-alias-int-range`, done before this).
- Removing the aliases from the core, schema, bindings or registry kind list
  (`go-uint-alias-removal`).
- TS integer typing.
- Go `complex64`/`complex128`.
- Classifying unmarshal failures as input rejections (str-4yc9w).

## Size

M, at the upper end. Roughly: analyzer and `TypeInfo` (2 files plus tests), planner (2 files plus
tests), handler (1 file), registry, parity matrix, handshake golden and conformance cases, the
fixture and checker, one E2E test, and `shatter-go/CLAUDE.md`. No core source changes. If the
planner/handler adaptation proves larger than expected, split it out and keep the analyzer change
behind it, not the other way round: emitting the new TypeInfo before the planner understands it
would regress `[]byte` handling.

## References

- `core-go-alias-int-range`: prerequisite; makes the core treat both wire forms as unsigned ints.
- str-cfsa (closed): introduced `go_uint`.
- str-ieuc (closed): `go_byte` coercion.
- str-79nvf (closed): `[]byte` detection.
- str-ddxe (closed): the protocol fields.
- str-4yc9w (open): Go decode-failure classification. Related, not blocking.
- No existing tracker issue covers Go int width emission (`bd search go_uint` and `int8` return
  nothing).
