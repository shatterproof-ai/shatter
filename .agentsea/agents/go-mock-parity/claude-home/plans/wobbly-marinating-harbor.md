# Plan: str-1o4m — Proptest coverage for input_gen mutation and shrink

## Context

The existing proptest coverage in `input_gen.rs` has only 4 trivial properties (3 type-preservation for `mutate_value` + 1 length-preservation for `mutate_inputs`). The `shrink.rs` module has only fixed unit tests, no proptest. We need comprehensive vector-level property tests for mutation, crossover, and shrink operations.

## Files to Modify

- **`shatter-core/src/input_gen.rs`** — Add proptest properties to the existing `prop_tests` module (~line 3089)
- **`shatter-core/src/shrink.rs`** — Add a `prop_tests` module with proptest properties

## Shared Generators to Reuse

From `shatter-core/src/test_arbitraries.rs`:
- `arb_type_info(depth)` — generates arbitrary `TypeInfo` (depth-bounded)
- `arb_param_info()` — generates `ParamInfo` with name + type
- `arb_json_value()` — small set of representative JSON values

## Helper Needed

A `value_matches_type(value: &Value, typ: &TypeInfo) -> bool` helper function for asserting type compatibility in property tests. This doesn't exist yet — I'll add it as a `#[cfg(test)]` helper in `input_gen.rs` tests since both modules' tests need it. It should check:
- `Int` → `is_i64() || is_u64()`
- `Float` → `is_f64() || is_i64()` (JSON integers are valid floats)
- `Str` → `is_string()`
- `Bool` → `is_boolean()`
- `Array` → `is_array()`
- `Object` → `is_object()`
- `Nullable` → `is_null() || matches inner`
- `Union` → matches any variant
- `Unknown/Complex/Opaque` → always true (anything goes)

## Properties to Add

### 1. `mutate_inputs` — vector-level properties (in `input_gen.rs::prop_tests`)

```
mutate_inputs_preserves_length_and_types:
  ∀ (params: Vec<ParamInfo>, seed: u64)
  generate inputs from params, then mutate with rate=1.0
  → output.len() == input.len()
  → each output[i] matches params[i].typ

mutate_inputs_actually_mutates:
  ∀ (seed: u64)
  fixed params [Int, Str, Bool], mutate with rate=1.0 over many seeds
  → at least one element differs from input (statistical: run 20 seeds, assert ≥1 differs)
```

### 2. `crossover_inputs` properties (in `input_gen.rs::prop_tests`)

```
crossover_inputs_preserves_length:
  ∀ (params: Vec<ParamInfo>, seed: u64)
  generate two parent vecs, crossover with rate=1.0
  → child1.len() == child2.len() == min(parent_a.len(), parent_b.len(), params.len())

crossover_inputs_type_compatible:
  ∀ (params: Vec<ParamInfo>, seed: u64)
  → each child element matches its param type
```

### 3. `shrink_candidates` properties (in `shrink.rs`)

```
shrink_never_contains_original:
  ∀ (value: Value, type_info: TypeInfo)
  → original ∉ shrink_candidates(value, type_info)

shrink_all_candidates_valid_type:
  ∀ (value: Value, type_info: TypeInfo)
  → all candidates match type_info

shrink_int_candidates_abs_leq_original:
  ∀ (n: i64)
  → all int candidates c satisfy |c| ≤ |n| or c ∈ {0, 1, -1}

shrink_string_candidates_len_leq_original:
  ∀ (s: String)
  → all string candidates c satisfy c.len() ≤ s.len()

shrink_array_candidates_len_leq_original:
  ∀ (arr: Vec<Value>)
  → all array candidates c satisfy c.len() ≤ arr.len()

shrink_minimal_values_produce_empty:
  for each minimal value (0, 0.0, "", false, null, [])
  → shrink_candidates returns empty or only alternatives (not simpler)
```

### 4. Generator-aware mutation (skip)

The issue mentions generator-aware mutation, but `mutate_inputs` doesn't use generators — it's pure type-based mutation. `generate_inputs_with_generators` uses generators for *generation* not mutation. No generator-aware mutation path exists to test.

### 5. Idempotent shrinking (in `shrink.rs`)

```
shrink_idempotent_for_minimals:
  0 → empty or only {1, -1}
  "" → empty
  false → empty
  null (Nullable) → empty
  [] → empty
```

## Strategy for Generating Type-Compatible Values

For properties that need `(Value, TypeInfo)` pairs, I'll create a strategy that first generates a `TypeInfo`, then generates a compatible `Value` using `generate_random_value` with a seeded RNG. This ensures the value actually conforms to the type.

## Verification

1. `cargo test -p shatter-core` — all tests pass
2. `cargo clippy -p shatter-core -- -D warnings` — no warnings
