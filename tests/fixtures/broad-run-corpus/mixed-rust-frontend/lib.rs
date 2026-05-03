// Rust source in a tree where the Rust frontend is unavailable.
// Shatter scan should not crash; it should either skip these files
// or report them with a frontend-unavailable reason.

pub fn classify(n: i32) -> &'static str {
    if n < 0 {
        "neg"
    } else if n == 0 {
        "zero"
    } else {
        "pos"
    }
}
