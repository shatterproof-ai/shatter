// Single-file Rust target. The broad-run gate invokes shatter against this
// fixture with PATH stripped (matching the technique used by
// shatter-cli/tests/rust_frontend_availability_test.rs) so the Rust frontend
// cannot be located. The CLI must surface `frontend_unavailable` /
// `skipped_by_unavailable_frontend` rather than panicking (str-jeen.13).

pub fn classify_temperature(degrees_celsius: i32) -> &'static str {
    if degrees_celsius < 0 {
        return "freezing";
    }
    if degrees_celsius < 20 {
        return "cold";
    }
    if degrees_celsius < 30 {
        return "warm";
    }
    "hot"
}
