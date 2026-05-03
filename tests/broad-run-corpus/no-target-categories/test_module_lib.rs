// NoTargetReason::TestModule — a Rust file consisting entirely of a
// `#[cfg(test)]`-gated module. The Rust frontend should classify this as
// `test_module`.

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        assert_eq!(1 + 1, 2);
    }
}
