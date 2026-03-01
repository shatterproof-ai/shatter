// Example 5a: Find First Negative
// Tests shatter's ability to explore loops with early return.
//
// EXPECTED BRANCHES (3):
//   1. empty slice                      -> None
//   2. slice with no negatives          -> None
//   3. slice with at least one negative -> Some(index of first negative)
//
// DIFFICULTY: Medium. Requires generating slices of varying lengths and values.
pub fn find_first_negative(values: &[i32]) -> Option<usize> {
    for (i, &v) in values.iter().enumerate() {
        if v < 0 {
            return Some(i);
        }
    }
    None
}

// Example 5b: Sum Until Threshold
// Tests shatter's ability to explore loop conditions with accumulator state.
//
// EXPECTED BRANCHES (3):
//   1. empty slice                          -> "empty"
//   2. sum reaches threshold before end     -> "threshold reached at index {i}"
//   3. sum never reaches threshold          -> "completed with sum {sum}"
pub fn sum_until_threshold(values: &[i32], threshold: i32) -> String {
    if values.is_empty() {
        return "empty".to_string();
    }

    let mut sum: i32 = 0;
    for (i, &v) in values.iter().enumerate() {
        sum = sum.saturating_add(v);
        if sum >= threshold {
            return format!("threshold reached at index {i}");
        }
    }
    format!("completed with sum {sum}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_first_negative_empty() {
        assert_eq!(find_first_negative(&[]), None);
    }

    #[test]
    fn test_find_first_negative_all_positive() {
        assert_eq!(find_first_negative(&[1, 2, 3]), None);
    }

    #[test]
    fn test_find_first_negative_found() {
        assert_eq!(find_first_negative(&[1, -2, 3]), Some(1));
    }

    #[test]
    fn test_sum_until_threshold_empty() {
        assert_eq!(sum_until_threshold(&[], 100), "empty");
    }

    #[test]
    fn test_sum_until_threshold_reached() {
        assert_eq!(
            sum_until_threshold(&[10, 20, 30, 40, 50], 55),
            "threshold reached at index 2"
        );
    }

    #[test]
    fn test_sum_until_threshold_not_reached() {
        assert_eq!(
            sum_until_threshold(&[1, 2, 3], 100),
            "completed with sum 6"
        );
    }
}
