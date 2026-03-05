/// classify_option — 3 branches: Some(n) where n>0 → "positive: {n}",
/// Some(n) where n<=0 → "non-positive: {n}", None → "absent".
pub fn classify_option(value: Option<i32>) -> String {
    match value {
        Some(n) if n > 0 => format!("positive: {n}"),
        Some(n) => format!("non-positive: {n}"),
        None => "absent".to_string(),
    }
}
