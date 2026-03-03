use extism_pdk::*;

/// WASM generator for DbConfig. Export name must match the type/param name
/// in .shatter/config.yaml.
///
/// Build: cargo build --target wasm32-wasip1 --release
/// Output: target/wasm32-wasip1/release/example_generators.wasm
#[plugin_fn]
pub fn DbConfig(input: String) -> FnResult<String> {
    let value = if input.is_empty() {
        // Fresh generation: produce a default config
        serde_json::json!({
            "host": "localhost",
            "port": 5432,
            "db": "test_wasm"
        })
    } else {
        // Reconstruction from recipe
        serde_json::from_str(&input)?
    };

    let result = serde_json::json!({
        "id": "wasm-postgres",
        "value": value
        // recipe omitted — for WASM generators, value IS the recipe
    });
    Ok(serde_json::to_string(&result)?)
}

/// Example parameter generator for auth tokens.
#[plugin_fn]
pub fn authToken(input: String) -> FnResult<String> {
    let token = if input.is_empty() {
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0In0.test".to_string()
    } else {
        input
    };

    let result = serde_json::json!({
        "id": "test-jwt",
        "value": token
    });
    Ok(serde_json::to_string(&result)?)
}
