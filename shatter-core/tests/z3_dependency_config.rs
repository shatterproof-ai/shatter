use std::fs;
use std::path::Path;

#[test]
fn z3_dependency_does_not_enable_gh_release() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let text = fs::read_to_string(&manifest).expect("read shatter-core Cargo.toml");

    assert!(
        !text.contains(r#"z3 = { version = "0.19", features = ["gh-release"] }"#),
        "shatter-core should not force z3 gh-release downloads in Cargo.toml"
    );
}
