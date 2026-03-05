/// classify_number — 4 branches: n<0 → "negative", n==0 → "zero",
/// n<=100 → "small positive", n>100 → "large positive".
pub fn classify_number(n: i64) -> &'static str {
    if n < 0 {
        "negative"
    } else if n == 0 {
        "zero"
    } else if n <= 100 {
        "small positive"
    } else {
        "large positive"
    }
}

/// classify_temperature — 4 branches: temp<0.0 → "freezing", temp<20.0 → "cold",
/// temp<35.0 → "comfortable", temp>=35.0 → "hot".
pub fn classify_temperature(temp: f64) -> &'static str {
    if temp < 0.0 {
        "freezing"
    } else if temp < 20.0 {
        "cold"
    } else if temp < 35.0 {
        "comfortable"
    } else {
        "hot"
    }
}
