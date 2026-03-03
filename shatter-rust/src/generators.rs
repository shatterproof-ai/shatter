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
pub type NativeGeneratorFn = Box<dyn Fn(Option<serde_json::Value>) -> GeneratorResult + Send + Sync>;

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
        self.handles.lock().expect("handle table lock poisoned").map.len()
    }
}

/// Registry dispatches generate requests to WASM plugins or native generators.
pub struct NativeRegistry {
    generators: HashMap<String, NativeGeneratorFn>,
    pub handles: HandleTable,
}

impl NativeRegistry {
    pub fn new() -> Self {
        Self {
            generators: HashMap::new(),
            handles: HandleTable::new(),
        }
    }

    /// Register a compiled-in generator function by name.
    pub fn register(&mut self, name: impl Into<String>, func: NativeGeneratorFn) {
        self.generators.insert(name.into(), func);
    }

    /// Look up and call a native generator by name.
    /// Returns (sentinel_json, generator_id, recipe) or an error.
    pub fn generate(
        &self,
        name: &str,
        recipe: Option<serde_json::Value>,
    ) -> Result<(serde_json::Value, String, serde_json::Value), String> {
        let func = self
            .generators
            .get(name)
            .ok_or_else(|| format!("native generator {name:?} not registered (custom build required)"))?;

        let result = func(recipe);
        let handle_id = self.handles.store(result.value);

        let sentinel = serde_json::json!({
            "__shatter_native": true,
            "handle": handle_id,
        });

        Ok((sentinel, result.id, result.recipe))
    }

    pub fn has_native(&self, name: &str) -> bool {
        self.generators.contains_key(name)
    }
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

        let (sentinel, id, recipe) = registry.generate("TestGen", None).expect("should work");
        assert_eq!(id, "test-gen");
        assert_eq!(recipe, serde_json::json!({"key": "val"}));
        assert_eq!(sentinel["__shatter_native"], true);
        assert!(sentinel["handle"].as_str().is_some());
    }

    #[test]
    fn native_registry_unregistered() {
        let registry = NativeRegistry::new();
        let err = registry.generate("Missing", None).unwrap_err();
        assert!(err.contains("not registered"));
    }
}
