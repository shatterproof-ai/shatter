//! Random input generation from TypeInfo metadata.
//!
//! Generates random JSON values matching the type signatures reported by
//! language frontends. Used for the initial exploration phase before symbolic
//! constraint solving kicks in.

use rand::Rng;
use serde_json::{json, Value};

use crate::types::TypeInfo;

/// Generate a random JSON value matching the given type.
///
/// Uses biased distributions that favor boundary values (0, -1, 1, empty
/// strings, etc.) to increase the chance of hitting interesting branches.
pub fn generate_random_value(typ: &TypeInfo, rng: &mut impl Rng) -> Value {
    match typ {
        TypeInfo::Int => generate_int(rng),
        TypeInfo::Float => generate_float(rng),
        TypeInfo::Str => generate_string(rng),
        TypeInfo::Bool => json!(rng.random_bool(0.5)),
        TypeInfo::Array { element } => generate_array(element, rng),
        TypeInfo::Object { fields } => generate_object(fields, rng),
        TypeInfo::Union { variants } => generate_union(variants, rng),
        TypeInfo::Nullable { inner } => generate_nullable(inner, rng),
        TypeInfo::Unknown => generate_unknown(rng),
    }
}

/// Generate a random integer, biased toward boundary values.
fn generate_int(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..10);
    let n = match choice {
        0 => 0,
        1 => 1,
        2 => -1,
        3 => i64::MAX,
        4 => i64::MIN,
        _ => rng.random_range(-1000..=1000),
    };
    json!(n)
}

/// Generate a random float, biased toward boundary values.
///
/// Includes integer values in the distribution since TypeScript's `number`
/// type covers both integers and floats.
fn generate_float(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..12);
    let n: f64 = match choice {
        0 => 0.0,
        1 => 1.0,
        2 => -1.0,
        3 => 0.5,
        4 => -0.5,
        // Include some integer values to cover integer-like branches (e.g. n % 2 === 0)
        5 => rng.random_range(-100..=100) as f64,
        6 => 2.0,
        7 => -2.0,
        8 => 10.0,
        _ => rng.random_range(-1000.0..1000.0),
    };
    json!(n)
}

/// Generate a random string from a small vocabulary plus random characters.
fn generate_string(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..10);
    let s = match choice {
        0 => String::new(),
        1 => "hello".to_string(),
        2 => "test".to_string(),
        3 => " ".to_string(),
        4 => "0".to_string(),
        5 => "true".to_string(),
        6 => "null".to_string(),
        _ => {
            let len = rng.random_range(1..=20);
            (0..len)
                .map(|_| rng.random_range(b'a'..=b'z') as char)
                .collect()
        }
    };
    json!(s)
}

/// Generate a random array with bounded length, biased toward small sizes.
///
/// Most real bugs appear at boundary lengths (0, 1, 2, 3), so we heavily
/// favor those over larger sizes. The distribution:
/// - 25% chance of length 0 (empty array)
/// - 25% chance of length 1 (single element)
/// - 20% chance of length 2
/// - 15% chance of length 3
/// - 15% chance of length 4-5 (larger arrays, less common in bug-triggering)
fn generate_array(element: &TypeInfo, rng: &mut impl Rng) -> Value {
    let len = generate_bounded_array_length(rng);
    let items: Vec<Value> = (0..len)
        .map(|_| generate_random_value(element, rng))
        .collect();
    json!(items)
}

/// Generate an array length biased toward small boundary values (0-3).
fn generate_bounded_array_length(rng: &mut impl Rng) -> usize {
    let choice: u8 = rng.random_range(0..20);
    match choice {
        0..5 => 0,   // 25%: empty
        5..10 => 1,  // 25%: single element
        10..14 => 2, // 20%: two elements
        14..17 => 3, // 15%: three elements
        _ => rng.random_range(4..=5), // 15%: larger
    }
}

/// Generate a random object with the specified fields.
fn generate_object(fields: &[(String, TypeInfo)], rng: &mut impl Rng) -> Value {
    let mut obj = serde_json::Map::new();
    for (name, typ) in fields {
        obj.insert(name.clone(), generate_random_value(typ, rng));
    }
    Value::Object(obj)
}

/// Pick a random variant from a union type.
fn generate_union(variants: &[TypeInfo], rng: &mut impl Rng) -> Value {
    if variants.is_empty() {
        return Value::Null;
    }
    let idx = rng.random_range(0..variants.len());
    generate_random_value(&variants[idx], rng)
}

/// Generate null ~30% of the time, otherwise generate the inner type.
fn generate_nullable(inner: &TypeInfo, rng: &mut impl Rng) -> Value {
    if rng.random_range(0..10) < 3 {
        Value::Null
    } else {
        generate_random_value(inner, rng)
    }
}

/// For unknown types, generate a random value from any primitive type.
fn generate_unknown(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..4);
    match choice {
        0 => generate_int(rng),
        1 => generate_float(rng),
        2 => generate_string(rng),
        3 => json!(rng.random_bool(0.5)),
        _ => unreachable!(),
    }
}

/// Generate a complete set of random inputs for a function's parameters.
pub fn generate_random_inputs(params: &[crate::types::ParamInfo], rng: &mut impl Rng) -> Vec<Value> {
    params.iter().map(|p| generate_random_value(&p.typ, rng)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ParamInfo;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn seeded_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn generates_int_values() {
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Int, &mut rng);
            assert!(val.is_i64() || val.is_u64(), "expected integer, got {val}");
        }
    }

    #[test]
    fn generates_float_values() {
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Float, &mut rng);
            assert!(val.is_f64() || val.is_i64(), "expected number, got {val}");
        }
    }

    #[test]
    fn generates_string_values() {
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Str, &mut rng);
            assert!(val.is_string(), "expected string, got {val}");
        }
    }

    #[test]
    fn generates_bool_values() {
        let mut rng = seeded_rng();
        let mut saw_true = false;
        let mut saw_false = false;
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Bool, &mut rng);
            assert!(val.is_boolean(), "expected bool, got {val}");
            if val.as_bool() == Some(true) {
                saw_true = true;
            } else {
                saw_false = true;
            }
        }
        assert!(saw_true && saw_false, "expected both true and false values");
    }

    #[test]
    fn generates_array_values() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        for _ in 0..20 {
            let val = generate_random_value(&typ, &mut rng);
            assert!(val.is_array(), "expected array, got {val}");
        }
    }

    #[test]
    fn generates_object_values() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                ("age".into(), TypeInfo::Int),
            ],
        };
        for _ in 0..20 {
            let val = generate_random_value(&typ, &mut rng);
            let obj = val.as_object().expect("expected object");
            assert!(obj.contains_key("name"));
            assert!(obj.contains_key("age"));
        }
    }

    #[test]
    fn generates_union_values() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Union {
            variants: vec![TypeInfo::Int, TypeInfo::Str],
        };
        let mut saw_int = false;
        let mut saw_str = false;
        for _ in 0..100 {
            let val = generate_random_value(&typ, &mut rng);
            if val.is_string() {
                saw_str = true;
            } else {
                saw_int = true;
            }
        }
        assert!(saw_int && saw_str, "expected both int and string variants");
    }

    #[test]
    fn generates_nullable_values_including_null() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Int),
        };
        let mut saw_null = false;
        let mut saw_value = false;
        for _ in 0..100 {
            let val = generate_random_value(&typ, &mut rng);
            if val.is_null() {
                saw_null = true;
            } else {
                saw_value = true;
            }
        }
        assert!(saw_null && saw_value, "expected both null and non-null values");
    }

    #[test]
    fn empty_union_produces_null() {
        let mut rng = seeded_rng();
        let val = generate_random_value(
            &TypeInfo::Union { variants: vec![] },
            &mut rng,
        );
        assert!(val.is_null());
    }

    #[test]
    fn generate_random_inputs_matches_param_count() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str },
            ParamInfo { name: "c".into(), typ: TypeInfo::Bool },
        ];
        let inputs = generate_random_inputs(&params, &mut rng);
        assert_eq!(inputs.len(), 3);
    }

    #[test]
    fn bounded_array_length_favors_small_sizes() {
        let mut rng = seeded_rng();
        let mut counts = [0u32; 6]; // indices 0-5
        let trials = 1000;
        for _ in 0..trials {
            let len = generate_bounded_array_length(&mut rng);
            assert!(len <= 5, "length should be at most 5, got {len}");
            counts[len] += 1;
        }
        // Empty (0) and single-element (1) should each be ~25% of results.
        // With 1000 trials, expect at least 150 each (well below 25%).
        assert!(
            counts[0] >= 150,
            "expected empty arrays to be common, got {}/{}",
            counts[0],
            trials
        );
        assert!(
            counts[1] >= 150,
            "expected single-element arrays to be common, got {}/{}",
            counts[1],
            trials
        );
        // Small sizes (0-3) should dominate: at least 75% of results.
        let small: u32 = counts[0] + counts[1] + counts[2] + counts[3];
        assert!(
            small >= 700,
            "expected small sizes (0-3) to dominate, got {small}/{trials}"
        );
    }

    #[test]
    fn generated_arrays_have_bounded_length() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        for _ in 0..100 {
            let val = generate_random_value(&typ, &mut rng);
            let arr = val.as_array().expect("expected array");
            assert!(arr.len() <= 5, "array too long: {}", arr.len());
        }
    }

    #[test]
    fn unknown_type_generates_diverse_values() {
        let mut rng = seeded_rng();
        let mut types_seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Unknown, &mut rng);
            if val.is_string() {
                types_seen.insert("string");
            } else if val.is_boolean() {
                types_seen.insert("bool");
            } else if val.is_number() {
                types_seen.insert("number");
            }
        }
        assert!(types_seen.len() >= 2, "expected diverse types for Unknown");
    }
}
