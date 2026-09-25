---
slug: go-int-width-sign-emission
kind: new
title: "Go analyzer: emit int_width/int_signed for every integer kind and demote go_uint/go_byte to deprecated aliases; int8/int16/uint16/byte params currently get out-of-range values"
priority: P2
type: bug
labels: [go-frontend, protocol, parity, input-generation, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [core-int-range-i128]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go analyzer: emit int_width/int_signed for every integer kind; demote go_uint/go_byte to aliases

Step 3 of epic `int-width-signedness-epic`. Removing the aliases is the separate deferred issue
`go-uint-alias-removal`. All paths are relative to the shatter repo root
(github.com/shatterproof-ai/shatter). Line numbers were verified at 16794cef.

## Problem

The protocol's `kind: "int"` carries `int_width`/`int_signed`
(`protocol/schemas/type-info.schema.json:18-25`), and the Rust frontend fills them in. The Go
analyzer does not.

**Signed kinds** (`int8`, `int16`, `int32`/`rune`, `int64`, `int`) map to a bare `{"kind":"int"}`
(`shatter-go/protocol/analyzer.go:1489`, `:1506` in `basicTypeInfo`; `:1561` in
`typeInfoFromAST`). The core treats them as full `i64`.

**Unsigned kinds** use Go-only complex kinds instead of the protocol fields:

- `uint`, `uint16`, `uint32`, `uint64` and `uintptr` map to `go_uint` (`:1502`, `:1565`);
- `uint8`/`byte` maps to `go_byte` (`:1496`, `:1558`).

Each has its own core generator and mutator (`shatter-core/src/input_gen.rs:677-678`, `:979`,
`:997`, `:2563-2564`, `:2976`, `:2992`) and serializer (`orchestrator.rs:1151-1175`). They all sit
outside the `int_range` path and the solver's range assertion (`solver.rs:172`). So `uint16` gets
`u32::MAX` and `u64::MAX`, and fixes to one path do not reach the other.

`go_uint`/`go_byte` are also referenced by:

- the Go planner: `shatter-go/planner/param.go:692-716` (uint family, and `[]byte` detection from
  str-79nvf) and `planner/composite.go:213-244`;
- the Go handler: `protocol/handler.go:306` (the `complex_type:go_byte` capability) and
  `:2227`, `:2337`;
- test export: `shatter-core/src/export.rs:513`, `:589`;
- `protocol/registry.yaml:370`, `:454`;
- `protocol/parity-matrix.yaml:458`;
- `protocol/conformance/golden/handshake/go.json:9`;
- the generated bindings `shatter-rust/src/protocol.rs:50` and `shatter-ts/src/protocol.ts:437`.
  Regenerate these from the schema; never hand-edit them.

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

## Baseline (run on main before changing code)

This needs no external examples checkout. Run from the repo root:

```bash
git rev-parse HEAD
cargo build -p shatter-cli && (cd shatter-go && go build -buildvcs=false -o bin/shatter-go .)
export PATH="$PWD/shatter-go/bin:$PATH"
tmp=$(mktemp -d) && cp -r examples/go/int-width "$tmp"/
target/debug/shatter explore "$tmp/int-width/widths.go" --allow-host-writes \
  --max-iterations 60 --request-timeout 240
python3 scripts/check_go_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
```

Expected at 16794cef (recorded 2026-09-24): the checker exits 1 with
`checked 360 executed inputs; out of bounds: 80`. Examples:

- `AtMaxInt8`: 128, 671, -303, `i64::MIN`, `i64::MAX`;
- `AtMaxUint16`: 65536, 4294967295, 18446744073709551615, `null`;
- `AtMaxByte`: 256;
- `Label`'s `int16`: `i64::MAX`, `i64::MIN`;
- `AtMaxUint64`: `null`.

In the report these appear as `throws function_error: param n: json: cannot unmarshal number 128
into Go value of type int8`. The min/max branches themselves are already reached on main, through
Z3 on the literals and through `go_uint`'s `u64::MAX`. That coverage must not regress.

## Acceptance criteria

- [ ] The fixture and checker are committed. The baseline output is pasted with
  `git rev-parse HEAD`.
- [ ] Both Go mapping sites (`basicTypeInfo` and `typeInfoFromAST`) emit
  `{"kind":"int","int_width":W,"int_signed":S}` for every Go integer kind. `int`, `uint` and
  `uintptr` use width 64, and the comment states the 64-bit-platform assumption. `rune` is
  `(32, true)`. `byte` is `(8, false)`.
- [ ] The planner and handler still recognize the uint family and `[]byte` from the new TypeInfo
  (`planner/param.go`, `planner/composite.go`, `handler.go:2227`, `:2337`). The existing
  str-79nvf and str-ieuc tests pass unchanged, and new cases cover the int-typed forms. Test export
  (`export.rs:513`, `:589`) still emits Go `byte`/`uint16` etc. from the new TypeInfo.
- [ ] **Deprecated aliases.** The core still accepts `go_uint`/`go_byte` on the wire and treats
  them as `Int { 64, false }` / `Int { 8, false }` on the `int_range` path, not through the
  separate generators. A deserialization test covers each alias. SPEC §8 gets a changelog row
  marking both deprecated, pointing to `go-uint-alias-removal`. The aliases exist so that an older
  installed `shatter-go` binary still works with a newer core. The Go frontend stops declaring
  `complex_type:go_byte`. Update `handler.go:306`, the handshake golden and `registry.yaml` to
  match.
- [ ] `protocol/parity-matrix.yaml` gains an "integer width and signedness" row: Rust and Go
  emit it, and TS is n/a because `number` is float and `bigint` is `big_int`. Update the `go_byte`
  row at `:458`. Add a conformance case per frontend that asserts the emitted TypeInfo for the
  fixture's params. `task parity` and `task conformance` pass.
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
  Paste the failing `test result:` line from main and the passing line from the branch.
- [ ] Re-run the baseline on the branch, once with the default explorer and once with
  `--concolic`. Both checker runs print `out of bounds: 0`.
- [ ] `shatter-go/CLAUDE.md` documents the protocol-visible change. `task e2e` and
  `task affected` pass, with `Gates selected` recorded.
- [ ] `go-uint-alias-removal` is filed (it is part of this epic's drafts). Its id goes in this
  issue's close reason. This issue closes on its own criteria and does not wait for any release.

## Out of scope

- Removing the aliases (`go-uint-alias-removal`).
- TS integer typing.
- Go `complex64`/`complex128`.
- Classifying unmarshal failures as input rejections (str-4yc9w).

## Size

M

## References

- str-cfsa (closed): introduced `go_uint`.
- str-ieuc (closed): `go_byte` coercion.
- str-79nvf (closed): `[]byte` detection.
- str-ddxe (closed): the protocol fields.
- str-4yc9w (open): Go decode-failure classification. Related, not blocking.
- `core-int-range-i128`: the prerequisite. Moving Go to the int path before `u64::MAX` is
  representable would regress `AtMaxUint64`.
- No existing tracker issue covers Go int width emission (`bd search go_uint` and `int8` return
  nothing).
