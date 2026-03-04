// Example 3a: Describe Shape
// Tests shatter's ability to explore custom enum variants with associated data.
//
// EXPECTED BRANCHES (4):
//   1. Shape::Circle { radius } where radius > 0  -> "circle with radius {radius}"
//   2. Shape::Circle { radius } where radius <= 0 -> "invalid circle"
//   3. Shape::Rectangle { width, height }          -> "rectangle {width}x{height}"
//   4. Shape::Triangle { base, height }            -> "triangle base={base} height={height}"

pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

pub fn describe_shape(shape: &Shape) -> String {
    match shape {
        Shape::Circle { radius } if *radius > 0.0 => {
            format!("circle with radius {radius}")
        }
        Shape::Circle { .. } => "invalid circle".to_string(),
        Shape::Rectangle { width, height } => {
            format!("rectangle {width}x{height}")
        }
        Shape::Triangle { base, height } => {
            format!("triangle base={base} height={height}")
        }
    }
}

// Example 3b: Classify Result
// Tests shatter's ability to explore Result<T, E> variants.
//
// EXPECTED BRANCHES (2):
//   1. Ok(value)  -> "ok: {value}"
//   2. Err(error) -> "error: {error}"
pub fn classify_result(result: &Result<String, String>) -> String {
    match result {
        Ok(value) => format!("ok: {value}"),
        Err(error) => format!("error: {error}"),
    }
}

// Example 3c: Classify Option
// Tests shatter's ability to explore Option<T> variants.
//
// EXPECTED BRANCHES (3):
//   1. Some(n) where n > 0 -> "positive: {n}"
//   2. Some(n) where n <= 0 -> "non-positive: {n}"
//   3. None                -> "absent"
pub fn classify_option(opt: Option<i32>) -> String {
    match opt {
        Some(n) if n > 0 => format!("positive: {n}"),
        Some(n) => format!("non-positive: {n}"),
        None => "absent".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describe_shape_valid_circle() {
        let shape = Shape::Circle { radius: 5.0 };
        assert_eq!(describe_shape(&shape), "circle with radius 5");
    }

    #[test]
    fn test_describe_shape_invalid_circle() {
        let shape = Shape::Circle { radius: -1.0 };
        assert_eq!(describe_shape(&shape), "invalid circle");
    }

    #[test]
    fn test_describe_shape_rectangle() {
        let shape = Shape::Rectangle {
            width: 3.0,
            height: 4.0,
        };
        assert_eq!(describe_shape(&shape), "rectangle 3x4");
    }

    #[test]
    fn test_describe_shape_triangle() {
        let shape = Shape::Triangle {
            base: 6.0,
            height: 2.0,
        };
        assert_eq!(describe_shape(&shape), "triangle base=6 height=2");
    }

    #[test]
    fn test_classify_result_ok() {
        let r: Result<String, String> = Ok("data".to_string());
        assert_eq!(classify_result(&r), "ok: data");
    }

    #[test]
    fn test_classify_result_err() {
        let r: Result<String, String> = Err("fail".to_string());
        assert_eq!(classify_result(&r), "error: fail");
    }

    #[test]
    fn test_classify_option_positive() {
        assert_eq!(classify_option(Some(42)), "positive: 42");
    }

    #[test]
    fn test_classify_option_non_positive() {
        assert_eq!(classify_option(Some(-3)), "non-positive: -3");
    }

    #[test]
    fn test_classify_option_none() {
        assert_eq!(classify_option(None), "absent");
    }
}
