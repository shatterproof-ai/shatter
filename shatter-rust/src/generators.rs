use std::collections::HashMap;
use std::sync::Mutex;

/// Result returned by compiled-in (custom build) native generators.
/// Value holds a live in-process object; recipe is a serializable blob for replay.
pub struct GeneratorResult {
    pub id: String,
    pub value: Box<dyn std::any::Any + Send>,
    pub recipe: serde_json::Value,
}

/// Signature for custom-build native generator functions.
pub type NativeGeneratorFn =
    Box<dyn Fn(Option<serde_json::Value>) -> GeneratorResult + Send + Sync>;

/// Stores live (non-serializable) objects returned by native generators.
/// Core receives a sentinel JSON referencing the handle; the frontend resolves
/// it to the live object during execution.
pub struct HandleTable {
    handles: Mutex<HandleTableInner>,
}

struct HandleTableInner {
    map: HashMap<String, Box<dyn std::any::Any + Send>>,
    next_id: u32,
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HandleTable {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HandleTableInner {
                map: HashMap::new(),
                next_id: 0,
            }),
        }
    }

    /// Store a live value and return a unique handle ID.
    pub fn store(&self, val: Box<dyn std::any::Any + Send>) -> String {
        let mut inner = self.handles.lock().expect("handle table lock poisoned");
        inner.next_id += 1;
        let id = format!("h_{:04}", inner.next_id);
        inner.map.insert(id.clone(), val);
        id
    }

    /// Resolve a handle to its live value, removing it from the table.
    pub fn take(&self, id: &str) -> Option<Box<dyn std::any::Any + Send>> {
        let mut inner = self.handles.lock().expect("handle table lock poisoned");
        inner.map.remove(id)
    }

    /// Clear all handles. Call between exploration runs.
    pub fn clear(&self) {
        let mut inner = self.handles.lock().expect("handle table lock poisoned");
        inner.map.clear();
        inner.next_id = 0;
    }

    /// Number of stored handles.
    pub fn len(&self) -> usize {
        self.handles
            .lock()
            .expect("handle table lock poisoned")
            .map
            .len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Registry dispatches generate requests to WASM plugins or native generators.
pub struct NativeRegistry {
    generators: HashMap<String, NativeGeneratorFn>,
    file_generators: HashMap<(String, String), NativeGeneratorFn>,
    pub handles: HandleTable,
}

impl Default for NativeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeRegistry {
    pub fn new() -> Self {
        Self {
            generators: HashMap::new(),
            file_generators: HashMap::new(),
            handles: HandleTable::new(),
        }
    }

    /// Register a compiled-in generator function by name.
    pub fn register(&mut self, name: impl Into<String>, func: NativeGeneratorFn) {
        self.generators.insert(name.into(), func);
    }

    /// Register a compiled-in generator function for a specific generator file.
    pub fn register_for_file(
        &mut self,
        file: impl Into<String>,
        name: impl Into<String>,
        func: NativeGeneratorFn,
    ) {
        self.file_generators
            .insert((normalize_generator_file(&file.into()), name.into()), func);
    }

    /// Look up and call a native generator by name.
    /// Returns (sentinel_json, generator_id, recipe) or an error.
    pub fn generate(
        &self,
        file: Option<&str>,
        name: &str,
        recipe: Option<serde_json::Value>,
    ) -> Result<(serde_json::Value, String, serde_json::Value), String> {
        let func = file
            .and_then(|file| {
                self.file_generators
                    .get(&(normalize_generator_file(file), name.to_string()))
            })
            .or_else(|| self.generators.get(name))
            .ok_or_else(|| {
                if let Some(file) = file {
                    format!(
                        "native generator {name:?} from {file:?} not registered (custom build required)"
                    )
                } else {
                    format!(
                        "native generator {name:?} not registered (custom build required)"
                    )
                }
            })?;

        let result = func(recipe);
        let handle_id = self.handles.store(result.value);

        let sentinel = serde_json::json!({
            "__shatter_native": true,
            "handle": handle_id,
        });

        Ok((sentinel, result.id, result.recipe))
    }

    /// Register all built-in native generators (currently: FileHandle).
    pub fn register_builtins(&mut self) {
        self.register(
            "FileHandle",
            Box::new(crate::file_handle_generator::generate),
        );
    }

    pub fn has_native(&self, name: &str) -> bool {
        self.generators.contains_key(name)
    }
}

fn normalize_generator_file(file: &str) -> String {
    std::fs::canonicalize(file)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| file.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_table_store_and_resolve() {
        let table = HandleTable::new();
        let id = table.store(Box::new(42_i32));
        assert_eq!(table.len(), 1);

        let val = table.take(&id).expect("should resolve");
        let n = val.downcast::<i32>().expect("should be i32");
        assert_eq!(*n, 42);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn handle_table_clear() {
        let table = HandleTable::new();
        table.store(Box::new("a"));
        table.store(Box::new("b"));
        assert_eq!(table.len(), 2);
        table.clear();
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn native_registry_generate() {
        let mut registry = NativeRegistry::new();
        registry.register(
            "TestGen",
            Box::new(|_recipe| GeneratorResult {
                id: "test-gen".into(),
                value: Box::new(String::from("live-value")),
                recipe: serde_json::json!({"key": "val"}),
            }),
        );

        let (sentinel, id, recipe) = registry
            .generate(None, "TestGen", None)
            .expect("should work");
        assert_eq!(id, "test-gen");
        assert_eq!(recipe, serde_json::json!({"key": "val"}));
        assert_eq!(sentinel["__shatter_native"], true);
        assert!(sentinel["handle"].as_str().is_some());
    }

    #[test]
    fn native_registry_unregistered() {
        let registry = NativeRegistry::new();
        let err = registry.generate(None, "Missing", None).unwrap_err();
        assert!(err.contains("not registered"));
    }

    #[test]
    fn native_registry_dispatches_by_file_when_available() {
        let mut registry = NativeRegistry::new();
        registry.register(
            "current",
            Box::new(|_recipe| GeneratorResult {
                id: "default".into(),
                value: Box::new(String::from("default-value")),
                recipe: serde_json::json!({"source": "default"}),
            }),
        );
        registry.register_for_file(
            "generators/bundle.rs",
            "current",
            Box::new(|_recipe| GeneratorResult {
                id: "bundle".into(),
                value: Box::new(String::from("bundle-value")),
                recipe: serde_json::json!({"source": "bundle"}),
            }),
        );

        let (_, id, recipe) = registry
            .generate(Some("generators/bundle.rs"), "current", None)
            .expect("file-specific generator should run");
        assert_eq!(id, "bundle");
        assert_eq!(recipe, serde_json::json!({"source": "bundle"}));

        let (_, id, recipe) = registry
            .generate(Some("generators/other.rs"), "current", None)
            .expect("name fallback should run");
        assert_eq!(id, "default");
        assert_eq!(recipe, serde_json::json!({"source": "default"}));
    }
}
