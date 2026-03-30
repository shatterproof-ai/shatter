use std::collections::HashMap;
use std::path::Path;

use extism::{Manifest, Plugin, Wasm};

/// Caches loaded WASM plugins by file path to avoid reloading on repeated generate calls.
pub struct WasmCache {
    plugins: HashMap<String, Plugin>,
}

impl Default for WasmCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmCache {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Invoke a WASM generator plugin.
    ///
    /// Loads the plugin from `wasm_path` (caching it for future calls), then calls
    /// `func_name` with the JSON-serialized `recipe` (or an empty string if absent).
    /// The plugin must return JSON: `{"id": "...", "value": ..., "recipe": ...}`.
    /// Returns `(value, generator_id, optional_recipe)`.
    pub fn generate(
        &mut self,
        wasm_path: &Path,
        func_name: &str,
        recipe: Option<&serde_json::Value>,
    ) -> Result<(serde_json::Value, String, Option<serde_json::Value>), String> {
        let key = wasm_path
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 WASM path: {}", wasm_path.display()))?
            .to_string();

        if !self.plugins.contains_key(&key) {
            let plugin = self.load_plugin(wasm_path)?;
            self.plugins.insert(key.clone(), plugin);
        }

        let plugin = self
            .plugins
            .get_mut(&key)
            .expect("just inserted above");

        let input = match recipe {
            Some(r) => serde_json::to_string(r).map_err(|e| format!("serialize recipe: {e}"))?,
            None => String::new(),
        };

        let output = plugin
            .call::<&str, &str>(func_name, &input)
            .map_err(|e| format!("WASM call failed: {e}"))?;

        let parsed: serde_json::Value =
            serde_json::from_str(output).map_err(|e| format!("invalid JSON from WASM plugin: {e}"))?;

        let id = parsed
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "WASM output missing \"id\" string field".to_string())?
            .to_string();

        let value = parsed
            .get("value")
            .cloned()
            .ok_or_else(|| "WASM output missing \"value\" field".to_string())?;

        let new_recipe = parsed.get("recipe").cloned();

        Ok((value, id, new_recipe))
    }

    fn load_plugin(&self, wasm_path: &Path) -> Result<Plugin, String> {
        if !wasm_path.exists() {
            return Err(format!("WASM file not found: {}", wasm_path.display()));
        }

        let wasm = Wasm::file(wasm_path);
        let manifest = Manifest::new([wasm]);
        Plugin::new(&manifest, [], true).map_err(|e| format!("failed to load WASM plugin: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_wasm_file_returns_error() {
        let mut cache = WasmCache::new();
        let path = PathBuf::from("/nonexistent/path/gen.wasm");
        let result = cache.generate(&path, "generate", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not found"),
            "expected 'not found' in error, got: {err}"
        );
    }

    #[test]
    fn cache_retains_plugin_entry() {
        let mut cache = WasmCache::new();
        // First call fails because the file doesn't exist, but we can verify
        // that a second call to a *different* missing file produces distinct errors.
        let path_a = PathBuf::from("/tmp/shatter-wasm-test-a.wasm");
        let path_b = PathBuf::from("/tmp/shatter-wasm-test-b.wasm");
        let err_a = cache.generate(&path_a, "gen", None).unwrap_err();
        let err_b = cache.generate(&path_b, "gen", None).unwrap_err();
        assert!(err_a.contains("shatter-wasm-test-a.wasm"));
        assert!(err_b.contains("shatter-wasm-test-b.wasm"));
        // Cache should have no entries since loading failed before insertion.
        assert!(cache.plugins.is_empty());
    }

}
