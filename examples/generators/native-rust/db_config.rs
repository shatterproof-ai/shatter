// Example native generator for Rust custom builds.
//
// Usage: place at .shatter/generators/db_config.rs and reference in config.yaml:
//
//   defaults:
//     generators:
//       DbConfig: .shatter/generators/db_config.rs
//
// Then build: shatter build-frontend rust

use shatter_rust::generators::GeneratorResult;

/// DbConfig generates database configuration objects. On fresh generation
/// (recipe is None), it creates a randomized test database name. On replay,
/// it reconstructs from the stored recipe.
pub fn DbConfig(recipe: Option<serde_json::Value>) -> GeneratorResult {
    let cfg = match recipe {
        Some(r) => r,
        None => {
            let db_name = format!("test_{:04}", rand::random::<u16>() % 10000);
            serde_json::json!({
                "host": "localhost",
                "port": 5432,
                "db": db_name
            })
        }
    };

    GeneratorResult {
        id: "local-postgres".into(),
        value: Box::new(cfg.clone()),
        recipe: cfg,
    }
}
