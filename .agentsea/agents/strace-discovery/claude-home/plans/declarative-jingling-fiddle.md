# Plan: shrink_candidates() — type-aware input simplification (str-87k2)

## Context

The input_gen module has `mutate_value` and `crossover_inputs` for making values more novel/complex. `shrink_candidates` is the inverse — given a value and its type, produce progressively simpler variants for minimal witness shrinking and boundary refinement.

## Implementation

**File:** `shatter-core/src/input_gen.rs`

Add `pub fn shrink_candidates(value: &Value, type_info: &TypeInfo) -> Vec<Value>` after the crossover section (~line 1614), before `literals_to_candidate_inputs`. Follow the same pattern as `mutate_value` — top-level match on TypeInfo dispatching to private per-type helpers.

### Shrink strategies

Each helper returns a `Vec<Value>` of candidates (no duplicates, no identity):

| Type | Helper | Candidates |
|------|--------|------------|
| `Int` | `shrink_int` | halve toward zero (`n/2`), 0, 1, -1 (skip if equal to input) |
| `Float` | `shrink_float` | halve toward zero (`n/2.0`), 0.0, 1.0, -1.0 (skip if equal/NaN) |
| `Str` | `shrink_str` | remove last char, remove first char, empty string `""`, first char only (skip if already empty) |
| `Bool` | `shrink_bool` | `[false]` if true, `[]` if already false |
| `Array` | `shrink_array` | remove last element, remove first element, empty array `[]` (skip if already empty); recursively shrink each element |
| `Object` | `shrink_object` | remove each field one at a time; recursively shrink each field value |
| `Nullable` | `shrink_nullable` | `[null]` plus shrink of inner value |
| `Union` | `shrink_union` | shrink with each variant type, deduplicate |
| `Complex/Opaque/Unknown` | — | return `vec![]` (no shrink candidates) |

### Key design decisions

- **No RNG needed** — shrinking is deterministic (unlike mutation)
- **Filter identity** — never include the original value in candidates
- **Recursive but bounded** — array/object shrink recurse into elements but only one level deep (each element produces its own candidates, not nested shrinking)
- **Dedup** — use a simple `dedup` pass since candidates are small vectors

### Proptest coverage

Add to the existing `prop_tests` module:

1. **Type preservation** — `shrink_candidates(v, t)` returns only values matching `value_matches_type(_, t)` (reuse existing helper)
2. **No identity** — no candidate equals the original value
3. **Non-empty for non-trivial** — Int/Str/Bool with non-minimal values produce at least one candidate
4. **Simplicity** — for Int, all candidates have `abs(candidate) <= abs(original)` (shrink toward zero)

### Unit tests

A few specific examples:
- `shrink_candidates(json!(42), &TypeInfo::Int)` → contains 21, 0, 1, -1
- `shrink_candidates(json!("hello"), &TypeInfo::Str)` → contains "hell", "ello", "", "h"
- `shrink_candidates(json!(false), &TypeInfo::Bool)` → empty vec
- `shrink_candidates(json!([1,2,3]), &TypeInfo::Array{element: Int})` → contains [2,3], [1,2], []

## Verification

1. `cargo test -p shatter-core` — all tests pass
2. `cargo clippy -p shatter-core -- -D warnings` — clean
3. No E2E/walkthrough needed (pure library function, not in pipeline path)
