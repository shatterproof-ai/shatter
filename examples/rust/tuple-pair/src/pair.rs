//! str-f4sow end-to-end fixture for Rust tuple-param input generation.
//!
//! `classify_pair`'s only param is a 2-tuple, which `shatter-rust`'s analyzer
//! maps to `TypeInfo::Object { fields: [("0", …), ("1", …)] }` (`TypeInfo` has
//! no tuple variant). The generated harness deserializes the param with
//! `serde_json::from_value::<(i64, i64)>`, which requires a JSON ARRAY. Before
//! str-f4sow, the core input generator seeded such params as JSON OBJECTS
//! (`{"0":…,"1":…}`), which the harness rejected outright — the function never
//! executed and no constraints were ever recorded for Z3 to solve.
pub fn classify_pair(p: (i64, i64)) -> &'static str {
    if p.0 == p.1 {
        "equal"
    } else if p.0 > p.1 {
        "greater"
    } else {
        "less"
    }
}
