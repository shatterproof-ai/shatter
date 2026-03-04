//! Static registry of known cryptographic API functions.
//!
//! Maps library functions (by language, package, symbol) to their
//! cryptographic role — direction (encrypt/decrypt), parameter roles,
//! and output semantics. Used during analysis to identify crypto
//! boundaries without runtime cost.
//!
//! The built-in registry is embedded at compile time. Users can extend
//! it via `.shatter/crypto-registry.toml`.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::ExternalDependency;

/// Built-in registry TOML, embedded at compile time.
const BUILTIN_REGISTRY: &str = include_str!("../data/crypto-registry.toml");

/// Direction of a cryptographic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CryptoDirection {
    Encrypt,
    Decrypt,
    /// Authenticated encryption or stream ciphers that do both.
    Both,
}

/// Role of a function parameter in a cryptographic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamRole {
    Key,
    Data,
    Iv,
    Nonce,
    Tag,
    Aad,
    Algorithm,
}

/// What the function's return value represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputSemantics {
    Ciphertext,
    Plaintext,
    Key,
    Hash,
    Signature,
    Verified,
}

/// A single entry in the crypto registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoEntry {
    /// Target language: "typescript", "go", or "rust".
    pub language: String,
    /// Module/package path as reported by frontend analyzers.
    pub package: String,
    /// Function or method name within the package.
    pub symbol: String,
    /// Whether this function encrypts, decrypts, or both.
    pub direction: CryptoDirection,
    /// Maps parameter position to its cryptographic role.
    #[serde(default)]
    pub param_roles: HashMap<String, ParamRole>,
    /// What the return value represents.
    pub output: OutputSemantics,
}

/// Deserialization wrapper for the TOML file.
#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    entries: Vec<CryptoEntry>,
}

/// Lookup key for fast registry access.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LookupKey {
    language: String,
    package: String,
    symbol: String,
}

/// Registry of known cryptographic API functions.
///
/// Entries are indexed by (language, package, symbol) for O(1) lookup.
/// The built-in registry is always loaded; user overrides merge on top
/// (matching entries are replaced, new entries are added).
#[derive(Debug)]
pub struct CryptoRegistry {
    entries: HashMap<LookupKey, CryptoEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("failed to parse registry TOML: {source}")]
    Parse {
        #[from]
        source: toml::de::Error,
    },
    #[error("failed to read registry file {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

impl CryptoRegistry {
    /// Load the built-in registry (embedded at compile time).
    pub fn load() -> Result<Self, RegistryError> {
        let file: RegistryFile = toml::from_str(BUILTIN_REGISTRY)?;
        let mut entries = HashMap::with_capacity(file.entries.len());
        for entry in file.entries {
            let key = LookupKey {
                language: entry.language.clone(),
                package: entry.package.clone(),
                symbol: entry.symbol.clone(),
            };
            entries.insert(key, entry);
        }
        Ok(Self { entries })
    }

    /// Load the built-in registry and merge entries from a user override file.
    ///
    /// Override entries with the same (language, package, symbol) replace
    /// built-in entries; new entries are added.
    pub fn load_with_override(path: &Path) -> Result<Self, RegistryError> {
        let mut registry = Self::load()?;
        let contents = std::fs::read_to_string(path).map_err(|e| RegistryError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let overrides: RegistryFile = toml::from_str(&contents)?;
        for entry in overrides.entries {
            let key = LookupKey {
                language: entry.language.clone(),
                package: entry.package.clone(),
                symbol: entry.symbol.clone(),
            };
            registry.entries.insert(key, entry);
        }
        Ok(registry)
    }

    /// Look up a crypto function by language, package, and symbol.
    pub fn lookup(&self, language: &str, package: &str, symbol: &str) -> Option<&CryptoEntry> {
        let key = LookupKey {
            language: language.to_string(),
            package: package.to_string(),
            symbol: symbol.to_string(),
        };
        self.entries.get(&key)
    }

    /// Check if an `ExternalDependency` matches a known crypto function.
    ///
    /// Tries to match against the registry using the dependency's
    /// `source_module` as package and `symbol` field. The symbol may be
    /// fully qualified (e.g. `"crypto.createDecipheriv"`), so we also
    /// try stripping the module prefix.
    pub fn classify_dependency(
        &self,
        dep: &ExternalDependency,
        language: &str,
    ) -> Option<&CryptoEntry> {
        // Try exact symbol match first.
        if let Some(entry) = self.lookup(language, &dep.source_module, &dep.symbol) {
            return Some(entry);
        }
        // Try stripping module prefix (e.g. "crypto.createDecipheriv" → "createDecipheriv").
        if let Some(suffix) = dep.symbol.strip_prefix(&dep.source_module) {
            let stripped = suffix.trim_start_matches('.');
            if !stripped.is_empty() {
                return self.lookup(language, &dep.source_module, stripped);
            }
        }
        None
    }

    /// Total number of entries in the registry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DependencyKind;

    #[test]
    fn load_builtin_registry() {
        let registry = CryptoRegistry::load().expect("built-in registry should parse");
        // Sanity check: we have entries for all three languages.
        assert!(registry.len() > 30, "expected 30+ entries, got {}", registry.len());

        let has_ts = registry.lookup("typescript", "crypto", "createDecipheriv").is_some();
        let has_go = registry.lookup("go", "crypto/cipher", "AEAD.Open").is_some();
        let has_rust = registry.lookup("rust", "aes-gcm", "Aes256Gcm::decrypt").is_some();
        assert!(has_ts, "missing TypeScript crypto entry");
        assert!(has_go, "missing Go crypto entry");
        assert!(has_rust, "missing Rust crypto entry");
    }

    #[test]
    fn lookup_node_crypto_decipheriv() {
        let registry = CryptoRegistry::load().unwrap();
        let entry = registry
            .lookup("typescript", "crypto", "createDecipheriv")
            .expect("createDecipheriv should exist");
        assert_eq!(entry.direction, CryptoDirection::Decrypt);
        assert_eq!(entry.output, OutputSemantics::Plaintext);
        assert_eq!(entry.param_roles.get("1"), Some(&ParamRole::Key));
        assert_eq!(entry.param_roles.get("2"), Some(&ParamRole::Iv));
    }

    #[test]
    fn lookup_go_aead_open() {
        let registry = CryptoRegistry::load().unwrap();
        let entry = registry
            .lookup("go", "crypto/cipher", "AEAD.Open")
            .expect("AEAD.Open should exist");
        assert_eq!(entry.direction, CryptoDirection::Decrypt);
        assert_eq!(entry.output, OutputSemantics::Plaintext);
    }

    #[test]
    fn lookup_go_aead_seal() {
        let registry = CryptoRegistry::load().unwrap();
        let entry = registry
            .lookup("go", "crypto/cipher", "AEAD.Seal")
            .expect("AEAD.Seal should exist");
        assert_eq!(entry.direction, CryptoDirection::Encrypt);
        assert_eq!(entry.output, OutputSemantics::Ciphertext);
    }

    #[test]
    fn lookup_rust_aes_gcm_decrypt() {
        let registry = CryptoRegistry::load().unwrap();
        let entry = registry
            .lookup("rust", "aes-gcm", "Aes256Gcm::decrypt")
            .expect("Aes256Gcm::decrypt should exist");
        assert_eq!(entry.direction, CryptoDirection::Decrypt);
        assert_eq!(entry.output, OutputSemantics::Plaintext);
    }

    #[test]
    fn lookup_rust_ring_seal() {
        let registry = CryptoRegistry::load().unwrap();
        let entry = registry
            .lookup("rust", "ring", "aead::seal_in_place")
            .expect("aead::seal_in_place should exist");
        assert_eq!(entry.direction, CryptoDirection::Encrypt);
        assert_eq!(entry.output, OutputSemantics::Ciphertext);
    }

    #[test]
    fn lookup_missing_returns_none() {
        let registry = CryptoRegistry::load().unwrap();
        assert!(registry.lookup("typescript", "crypto", "nonexistent").is_none());
        assert!(registry.lookup("python", "cryptography", "encrypt").is_none());
    }

    #[test]
    fn classify_dependency_exact_match() {
        let registry = CryptoRegistry::load().unwrap();
        let dep = ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: "createDecipheriv".to_string(),
            source_module: "crypto".to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![10],
        };
        let entry = registry
            .classify_dependency(&dep, "typescript")
            .expect("should match createDecipheriv");
        assert_eq!(entry.direction, CryptoDirection::Decrypt);
    }

    #[test]
    fn classify_dependency_with_module_prefix() {
        let registry = CryptoRegistry::load().unwrap();
        let dep = ExternalDependency {
            kind: DependencyKind::MethodCall,
            symbol: "crypto.createDecipheriv".to_string(),
            source_module: "crypto".to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![15],
        };
        let entry = registry
            .classify_dependency(&dep, "typescript")
            .expect("should match after stripping prefix");
        assert_eq!(entry.direction, CryptoDirection::Decrypt);
    }

    #[test]
    fn classify_dependency_no_match() {
        let registry = CryptoRegistry::load().unwrap();
        let dep = ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: "readFile".to_string(),
            source_module: "fs".to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![1],
        };
        assert!(registry.classify_dependency(&dep, "typescript").is_none());
    }

    #[test]
    fn override_merge() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let override_path = dir.path().join("crypto-override.toml");
        let mut f = std::fs::File::create(&override_path).unwrap();
        writeln!(
            f,
            r#"
[[entries]]
language = "typescript"
package = "crypto"
symbol = "createDecipheriv"
direction = "encrypt"
output = "ciphertext"

[[entries]]
language = "typescript"
package = "my-crypto-lib"
symbol = "customDecrypt"
direction = "decrypt"
output = "plaintext"
"#
        )
        .unwrap();

        let registry = CryptoRegistry::load_with_override(&override_path)
            .expect("override file should parse");

        // Overridden entry should have new direction.
        let overridden = registry
            .lookup("typescript", "crypto", "createDecipheriv")
            .expect("overridden entry should exist");
        assert_eq!(
            overridden.direction,
            CryptoDirection::Encrypt,
            "override should replace direction"
        );

        // New entry should be added.
        let custom = registry
            .lookup("typescript", "my-crypto-lib", "customDecrypt")
            .expect("custom entry should exist");
        assert_eq!(custom.direction, CryptoDirection::Decrypt);

        // Existing non-overridden entries should remain.
        assert!(registry.lookup("go", "crypto/cipher", "AEAD.Open").is_some());
    }

    #[test]
    fn crypto_js_entries() {
        let registry = CryptoRegistry::load().unwrap();
        let encrypt = registry
            .lookup("typescript", "crypto-js", "AES.encrypt")
            .expect("AES.encrypt should exist");
        assert_eq!(encrypt.direction, CryptoDirection::Encrypt);

        let decrypt = registry
            .lookup("typescript", "crypto-js", "AES.decrypt")
            .expect("AES.decrypt should exist");
        assert_eq!(decrypt.direction, CryptoDirection::Decrypt);
    }

    #[test]
    fn chacha20poly1305_entries() {
        let registry = CryptoRegistry::load().unwrap();

        let go_entry = registry
            .lookup("go", "golang.org/x/crypto/chacha20poly1305", "New")
            .expect("Go chacha20poly1305.New should exist");
        assert_eq!(go_entry.direction, CryptoDirection::Both);

        let rust_entry = registry
            .lookup("rust", "chacha20poly1305", "ChaCha20Poly1305::encrypt")
            .expect("Rust ChaCha20Poly1305::encrypt should exist");
        assert_eq!(rust_entry.direction, CryptoDirection::Encrypt);
    }
}
