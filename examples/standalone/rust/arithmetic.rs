// Example 1a: Classify Number
// Tests shatter's ability to find simple numeric boundary conditions.
//
// EXPECTED BRANCHES (4):
//   1. n < 0          -> "negative"
//   2. n == 0         -> "zero"
//   3. n > 0 && n <= 100 -> "small positive"
//   4. n > 100        -> "large positive"
pub fn classify_number(n: i32) -> &'static str {
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

// Example 1b: Classify Temperature
// Tests shatter's ability to handle floating-point boundary conditions.
//
// EXPECTED BRANCHES (4):
//   1. temp < 0.0     -> "freezing"
//   2. temp < 20.0    -> "cold"
//   3. temp < 35.0    -> "comfortable"
//   4. temp >= 35.0   -> "hot"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_number_negative() {
        assert_eq!(classify_number(-5), "negative");
    }

    #[test]
    fn test_classify_number_zero() {
        assert_eq!(classify_number(0), "zero");
    }

    #[test]
    fn test_classify_number_small_positive() {
        assert_eq!(classify_number(50), "small positive");
    }

    #[test]
    fn test_classify_number_large_positive() {
        assert_eq!(classify_number(200), "large positive");
    }

    #[test]
    fn test_classify_temperature_freezing() {
        assert_eq!(classify_temperature(-10.0), "freezing");
    }

    #[test]
    fn test_classify_temperature_cold() {
        assert_eq!(classify_temperature(10.0), "cold");
    }

    #[test]
    fn test_classify_temperature_comfortable() {
        assert_eq!(classify_temperature(25.0), "comfortable");
    }

    #[test]
    fn test_classify_temperature_hot() {
        assert_eq!(classify_temperature(40.0), "hot");
    }
}
