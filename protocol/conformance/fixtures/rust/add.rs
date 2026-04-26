// Conformance fixture for the rust frontend's outcome shape (str-hy9b.A5).
// A trivially executable free function so `execute` lands on the success path
// and the response carries `outcome.status = "completed"`.
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}
