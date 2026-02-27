//! Array-aware mutation strategies for coverage-directed fuzzing.
//!
//! Provides AFL-style structural mutations (splice, insert, delete, swap) for
//! array values, complementing the random input generation in [`crate::input_gen`].
//! These mutations operate on existing array inputs to explore nearby execution
//! paths without re-generating from scratch.

use rand::Rng;
use serde_json::Value;

use crate::input_gen::generate_random_value;
use crate::types::TypeInfo;

/// The kind of structural mutation applied to an array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrayMutationKind {
    /// Insert a new element at a random position.
    Insert,
    /// Delete an element at a random position.
    Delete,
    /// Swap two elements in the array.
    Swap,
    /// Replace a single element with a freshly generated value.
    Replace,
    /// Splice: replace a contiguous slice with new elements.
    Splice,
    /// Truncate the array to a smaller length.
    Truncate,
}

/// Result of mutating an array value.
#[derive(Debug)]
pub struct ArrayMutation {
    /// The mutated array value.
    pub value: Value,
    /// Which mutation was applied.
    pub kind: ArrayMutationKind,
}

/// Apply a random structural mutation to an array value.
///
/// Returns `None` if the input is not a JSON array. The `element_type` is used
/// when generating new elements (for insert, replace, splice).
pub fn mutate_array(
    value: &Value,
    element_type: &TypeInfo,
    rng: &mut impl Rng,
) -> Option<ArrayMutation> {
    let arr = value.as_array()?;
    let len = arr.len();

    // Choose a mutation appropriate for the current array size.
    let kind = choose_mutation(len, rng);
    let mutated = apply_mutation(arr, kind, element_type, rng);

    Some(ArrayMutation {
        value: Value::Array(mutated),
        kind,
    })
}

/// Choose a mutation kind appropriate for the array's current length.
fn choose_mutation(len: usize, rng: &mut impl Rng) -> ArrayMutationKind {
    match len {
        // Empty array: can only insert.
        0 => ArrayMutationKind::Insert,
        // Single element: insert, delete, or replace.
        1 => {
            let choice: u8 = rng.random_range(0..3);
            match choice {
                0 => ArrayMutationKind::Insert,
                1 => ArrayMutationKind::Delete,
                _ => ArrayMutationKind::Replace,
            }
        }
        // Two or more elements: all mutations available.
        _ => {
            let choice: u8 = rng.random_range(0..6);
            match choice {
                0 => ArrayMutationKind::Insert,
                1 => ArrayMutationKind::Delete,
                2 => ArrayMutationKind::Swap,
                3 => ArrayMutationKind::Replace,
                4 => ArrayMutationKind::Splice,
                _ => ArrayMutationKind::Truncate,
            }
        }
    }
}

/// Apply a specific mutation to an array, returning the mutated elements.
fn apply_mutation(
    arr: &[Value],
    kind: ArrayMutationKind,
    element_type: &TypeInfo,
    rng: &mut impl Rng,
) -> Vec<Value> {
    let mut result = arr.to_vec();

    match kind {
        ArrayMutationKind::Insert => {
            let pos = if result.is_empty() {
                0
            } else {
                rng.random_range(0..=result.len())
            };
            let new_elem = generate_random_value(element_type, rng);
            result.insert(pos, new_elem);
        }
        ArrayMutationKind::Delete => {
            if !result.is_empty() {
                let pos = rng.random_range(0..result.len());
                result.remove(pos);
            }
        }
        ArrayMutationKind::Swap => {
            if result.len() >= 2 {
                let i = rng.random_range(0..result.len());
                let mut j = rng.random_range(0..result.len() - 1);
                if j >= i {
                    j += 1;
                }
                result.swap(i, j);
            }
        }
        ArrayMutationKind::Replace => {
            if !result.is_empty() {
                let pos = rng.random_range(0..result.len());
                result[pos] = generate_random_value(element_type, rng);
            }
        }
        ArrayMutationKind::Splice => {
            if !result.is_empty() {
                let start = rng.random_range(0..result.len());
                let max_end = result.len().min(start + 3);
                let end = rng.random_range(start + 1..=max_end);
                let new_len = rng.random_range(0..=3);
                let new_elems: Vec<Value> = (0..new_len)
                    .map(|_| generate_random_value(element_type, rng))
                    .collect();
                result.splice(start..end, new_elems);
            }
        }
        ArrayMutationKind::Truncate => {
            if result.len() > 1 {
                let new_len = rng.random_range(0..result.len());
                result.truncate(new_len);
            }
        }
    }

    result
}

/// Generate a batch of mutated variants from a single array input.
///
/// Produces `count` mutations, useful for generating a diverse set of nearby
/// inputs from a single seed input that triggered an interesting path.
pub fn mutate_array_batch(
    value: &Value,
    element_type: &TypeInfo,
    count: usize,
    rng: &mut impl Rng,
) -> Vec<ArrayMutation> {
    (0..count)
        .filter_map(|_| mutate_array(value, element_type, rng))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use serde_json::json;

    fn seeded_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn mutate_returns_none_for_non_array() {
        let mut rng = seeded_rng();
        let result = mutate_array(&json!(42), &TypeInfo::Int, &mut rng);
        assert!(result.is_none());
    }

    #[test]
    fn mutate_empty_array_always_inserts() {
        let mut rng = seeded_rng();
        for _ in 0..20 {
            let result = mutate_array(&json!([]), &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            assert_eq!(result.kind, ArrayMutationKind::Insert);
            let arr = result.value.as_array().expect("should be array");
            assert_eq!(arr.len(), 1);
        }
    }

    #[test]
    fn insert_increases_length_by_one() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3]);
        // Run enough times to hit insert at least once.
        let mut saw_insert = false;
        for _ in 0..50 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            if result.kind == ArrayMutationKind::Insert {
                let arr = result.value.as_array().expect("should be array");
                assert_eq!(arr.len(), 4);
                saw_insert = true;
            }
        }
        assert!(saw_insert, "expected at least one Insert mutation");
    }

    #[test]
    fn delete_decreases_length_by_one() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3]);
        let mut saw_delete = false;
        for _ in 0..50 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            if result.kind == ArrayMutationKind::Delete {
                let arr = result.value.as_array().expect("should be array");
                assert_eq!(arr.len(), 2);
                saw_delete = true;
            }
        }
        assert!(saw_delete, "expected at least one Delete mutation");
    }

    #[test]
    fn swap_preserves_length_and_elements() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3]);
        let mut saw_swap = false;
        for _ in 0..50 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            if result.kind == ArrayMutationKind::Swap {
                let arr = result.value.as_array().expect("should be array");
                assert_eq!(arr.len(), 3);
                // Same elements, possibly reordered
                let mut sorted: Vec<i64> =
                    arr.iter().map(|v| v.as_i64().expect("int")).collect();
                sorted.sort();
                assert_eq!(sorted, vec![1, 2, 3]);
                saw_swap = true;
            }
        }
        assert!(saw_swap, "expected at least one Swap mutation");
    }

    #[test]
    fn replace_preserves_length() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3]);
        let mut saw_replace = false;
        for _ in 0..50 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            if result.kind == ArrayMutationKind::Replace {
                let arr = result.value.as_array().expect("should be array");
                assert_eq!(arr.len(), 3);
                saw_replace = true;
            }
        }
        assert!(saw_replace, "expected at least one Replace mutation");
    }

    #[test]
    fn truncate_reduces_length() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3, 4, 5]);
        let mut saw_truncate = false;
        for _ in 0..50 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            if result.kind == ArrayMutationKind::Truncate {
                let arr = result.value.as_array().expect("should be array");
                assert!(arr.len() < 5);
                saw_truncate = true;
            }
        }
        assert!(saw_truncate, "expected at least one Truncate mutation");
    }

    #[test]
    fn splice_modifies_contiguous_section() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3, 4]);
        let mut saw_splice = false;
        for _ in 0..50 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            if result.kind == ArrayMutationKind::Splice {
                // Splice can change length in either direction
                let arr = result.value.as_array().expect("should be array");
                assert!(arr.len() <= 6, "splice shouldn't produce huge arrays");
                saw_splice = true;
            }
        }
        assert!(saw_splice, "expected at least one Splice mutation");
    }

    #[test]
    fn batch_produces_requested_count() {
        let mut rng = seeded_rng();
        let original = json!([1, 2, 3]);
        let mutations = mutate_array_batch(&original, &TypeInfo::Int, 10, &mut rng);
        assert_eq!(mutations.len(), 10);
    }

    #[test]
    fn batch_with_non_array_returns_empty() {
        let mut rng = seeded_rng();
        let mutations = mutate_array_batch(&json!("not an array"), &TypeInfo::Int, 10, &mut rng);
        assert!(mutations.is_empty());
    }

    #[test]
    fn all_mutation_kinds_exercised_over_many_trials() {
        let mut rng = seeded_rng();
        let original = json!([10, 20, 30, 40, 50]);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            seen.insert(result.kind);
        }
        assert!(
            seen.contains(&ArrayMutationKind::Insert),
            "missing Insert"
        );
        assert!(
            seen.contains(&ArrayMutationKind::Delete),
            "missing Delete"
        );
        assert!(seen.contains(&ArrayMutationKind::Swap), "missing Swap");
        assert!(
            seen.contains(&ArrayMutationKind::Replace),
            "missing Replace"
        );
        assert!(
            seen.contains(&ArrayMutationKind::Splice),
            "missing Splice"
        );
        assert!(
            seen.contains(&ArrayMutationKind::Truncate),
            "missing Truncate"
        );
    }

    #[test]
    fn single_element_array_never_swaps_or_splices() {
        let mut rng = seeded_rng();
        let original = json!([42]);
        for _ in 0..100 {
            let result = mutate_array(&original, &TypeInfo::Int, &mut rng)
                .expect("should produce a mutation");
            assert_ne!(
                result.kind,
                ArrayMutationKind::Swap,
                "swap not valid for single element"
            );
            assert_ne!(
                result.kind,
                ArrayMutationKind::Splice,
                "splice not offered for single element"
            );
            assert_ne!(
                result.kind,
                ArrayMutationKind::Truncate,
                "truncate not offered for single element"
            );
        }
    }

    #[test]
    fn mutations_with_complex_element_type() {
        let mut rng = seeded_rng();
        let elem_type = TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                ("value".into(), TypeInfo::Int),
            ],
        };
        let original = json!([{"name": "a", "value": 1}]);
        for _ in 0..20 {
            let result = mutate_array(&original, &elem_type, &mut rng)
                .expect("should produce a mutation");
            let arr = result.value.as_array().expect("should be array");
            // All elements should be valid objects (existing or freshly generated)
            for elem in arr {
                if result.kind == ArrayMutationKind::Insert
                    || result.kind == ArrayMutationKind::Replace
                {
                    // New elements should be objects with the right fields
                    assert!(
                        elem.is_object(),
                        "expected object element, got {elem}"
                    );
                }
            }
        }
    }
}
