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

use crate::entropy::classify_entropy_delta;
use crate::nondeterminism::Confidence;
use crate::protocol::{CryptoBoundary, ExternalDependency};

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

// ---------------------------------------------------------------------------
// Layer 2: Naming heuristic patterns
// ---------------------------------------------------------------------------

/// Strong crypto name patterns — Medium confidence when matched.
/// Checked as case-insensitive substrings of the function/method name.
const STRONG_CRYPTO_PATTERNS: &[(&str, CryptoDirection)] = &[
    ("decrypt", CryptoDirection::Decrypt),
    ("decipher", CryptoDirection::Decrypt),
    ("encrypt", CryptoDirection::Encrypt),
    ("encipher", CryptoDirection::Encrypt),
    ("unseal", CryptoDirection::Decrypt),
    ("unprotect", CryptoDirection::Decrypt),
];

/// Ambiguous crypto patterns — Low confidence, only matched when the
/// dependency's `source_module` contains a crypto-related indicator.
const AMBIGUOUS_CRYPTO_PATTERNS: &[(&str, CryptoDirection)] = &[
    ("seal", CryptoDirection::Encrypt),
    ("open", CryptoDirection::Decrypt),
    ("protect", CryptoDirection::Encrypt),
];

/// Module name substrings that establish a crypto context for ambiguous patterns.
const CRYPTO_MODULE_INDICATORS: &[&str] = &[
    "crypto", "cipher", "aes", "ssl", "tls", "gpg", "nacl", "sodium", "bcrypt",
    "argon", "scrypt", "hmac", "rsa", "ecdsa", "ed25519", "chacha", "salsa",
    "blake", "tweetnacl", "libsodium",
];

/// Layer 2: classify a dependency by naming heuristic.
///
/// Matches function/method names against known crypto patterns. Strong patterns
/// (e.g. `decrypt*`, `encrypt*`) get Medium confidence; ambiguous patterns
/// (e.g. `open`, `seal`) get Low confidence and require the source module to
/// contain a crypto-related indicator.
///
/// Returns `None` if no pattern matches (or if the match is ambiguous without
/// crypto context).
pub fn classify_by_name(dep: &ExternalDependency) -> Option<CryptoBoundary> {
    let symbol_lower = dep.symbol.to_lowercase();

    // Strong patterns: match anywhere in the symbol name (case-insensitive).
    for &(pattern, direction) in STRONG_CRYPTO_PATTERNS {
        if symbol_lower.contains(pattern) {
            return Some(CryptoBoundary {
                symbol: dep.symbol.clone(),
                source_module: dep.source_module.clone(),
                direction,
                output: None,
                confidence: Confidence::Medium,
                param_roles: HashMap::new(),
                call_sites: dep.call_sites.clone(),
                input_entropy: None,
                output_entropy: None,
            });
        }
    }

    // Ambiguous patterns: require crypto context in the source module.
    let module_lower = dep.source_module.to_lowercase();
    let has_crypto_context = CRYPTO_MODULE_INDICATORS
        .iter()
        .any(|ind| module_lower.contains(ind));

    if has_crypto_context {
        for &(pattern, direction) in AMBIGUOUS_CRYPTO_PATTERNS {
            if symbol_lower.contains(pattern) {
                return Some(CryptoBoundary {
                    symbol: dep.symbol.clone(),
                    source_module: dep.source_module.clone(),
                    direction,
                    output: None,
                    confidence: Confidence::Low,
                    param_roles: HashMap::new(),
                    call_sites: dep.call_sites.clone(),
                    input_entropy: None,
                    output_entropy: None,
                });
            }
        }
    }

    None
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

    /// Classify all dependencies, returning `CryptoBoundary` for each match.
    ///
    /// Tries Layer 1 (exact registry match, High confidence) first, then
    /// falls back to Layer 2 (naming heuristic, Medium/Low confidence).
    pub fn classify_all_dependencies(
        &self,
        deps: &[ExternalDependency],
        language: &str,
    ) -> Vec<CryptoBoundary> {
        deps.iter()
            .filter_map(|dep| {
                // Layer 1: exact registry match.
                if let Some(entry) = self.classify_dependency(dep, language) {
                    return Some(CryptoBoundary {
                        symbol: dep.symbol.clone(),
                        source_module: dep.source_module.clone(),
                        direction: entry.direction,
                        output: Some(entry.output),
                        confidence: Confidence::High,
                        param_roles: entry.param_roles.clone(),
                        call_sites: dep.call_sites.clone(),
                        input_entropy: None,
                        output_entropy: None,
                    });
                }
                // Layer 2: naming heuristic.
                classify_by_name(dep)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Layer 3: Entropy-based confidence adjustment
// ---------------------------------------------------------------------------

/// Adjust a `CryptoBoundary`'s confidence using measured entropy values.
///
/// If entropy analysis confirms crypto behaviour (significant entropy delta),
/// the confidence is upgraded: Medium → High, Low → Medium.
/// If entropy contradicts (no significant delta), naming-heuristic results
/// (Medium/Low) are downgraded; Layer 1 High-confidence results are left
/// untouched since the static registry is authoritative.
pub fn confirm_with_entropy(
    boundary: &CryptoBoundary,
    input_entropy: f64,
    output_entropy: f64,
) -> CryptoBoundary {
    let entropy_direction = classify_entropy_delta(input_entropy, output_entropy);

    let new_confidence = match (boundary.confidence, entropy_direction.is_some()) {
        // Entropy confirms crypto → upgrade.
        (Confidence::Medium, true) => Confidence::High,
        (Confidence::Low, true) => Confidence::Medium,
        // High stays High regardless (Layer 1 is authoritative).
        (Confidence::High, _) => Confidence::High,
        // Entropy contradicts → downgrade naming-heuristic results.
        (Confidence::Medium, false) => Confidence::Low,
        (Confidence::Low, false) => Confidence::Low,
    };

    CryptoBoundary {
        confidence: new_confidence,
        input_entropy: Some(input_entropy),
        output_entropy: Some(output_entropy),
        ..boundary.clone()
    }
}

// ---------------------------------------------------------------------------
// Detection summary
// ---------------------------------------------------------------------------

/// Aggregated crypto detection results for a single function.
#[derive(Debug, Clone)]
pub struct CryptoDetectionSummary {
    /// All detected crypto boundaries.
    pub boundaries: Vec<CryptoBoundary>,
    /// Which detection layers contributed (1 = registry, 2 = naming heuristic).
    pub layers_used: Vec<u8>,
    /// Number of boundaries with High confidence.
    pub high_confidence_count: usize,
}

impl CryptoDetectionSummary {
    /// Build a summary from a list of classified boundaries.
    ///
    /// Layer assignment: boundaries with `output` semantics (set only by Layer 1)
    /// are tagged as layer 1; the rest come from Layer 2 naming heuristics.
    pub fn from_boundaries(boundaries: Vec<CryptoBoundary>) -> Self {
        let mut layers = Vec::new();
        let mut high_count = 0;

        for b in &boundaries {
            let layer: u8 = if b.output.is_some() { 1 } else { 2 };
            if !layers.contains(&layer) {
                layers.push(layer);
            }
            if b.confidence == Confidence::High {
                high_count += 1;
            }
        }

        layers.sort_unstable();

        Self {
            boundaries,
            layers_used: layers,
            high_confidence_count: high_count,
        }
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

    #[test]
    fn classify_all_dependencies_matches_crypto() {
        let registry = CryptoRegistry::load().unwrap();
        let deps = vec![
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "createDecipheriv".to_string(),
                source_module: "crypto".to_string(),
                return_type: crate::types::TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![5, 12],
            },
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "readFile".to_string(),
                source_module: "fs".to_string(),
                return_type: crate::types::TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![3],
            },
        ];

        let boundaries = registry.classify_all_dependencies(&deps, "typescript");
        assert_eq!(boundaries.len(), 1, "only the crypto dep should match");
        assert_eq!(boundaries[0].symbol, "createDecipheriv");
        assert_eq!(boundaries[0].source_module, "crypto");
        assert_eq!(boundaries[0].direction, CryptoDirection::Decrypt);
        assert_eq!(boundaries[0].output, Some(OutputSemantics::Plaintext));
        assert_eq!(boundaries[0].confidence, Confidence::High);
        assert_eq!(boundaries[0].call_sites, vec![5, 12]);
    }

    #[test]
    fn classify_all_dependencies_no_match() {
        let registry = CryptoRegistry::load().unwrap();
        let deps = vec![ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: "readFile".to_string(),
            source_module: "fs".to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![1],
        }];

        let boundaries = registry.classify_all_dependencies(&deps, "typescript");
        assert!(boundaries.is_empty());
    }

    #[test]
    fn classify_all_dependencies_empty_input() {
        let registry = CryptoRegistry::load().unwrap();
        let boundaries = registry.classify_all_dependencies(&[], "typescript");
        assert!(boundaries.is_empty());
    }

    // -- Layer 2: naming heuristic tests --

    fn make_dep(symbol: &str, source_module: &str) -> ExternalDependency {
        ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: symbol.to_string(),
            source_module: source_module.to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![10],
        }
    }

    #[test]
    fn classify_by_name_strong_pattern_encrypt() {
        let dep = make_dep("encryptData", "my-custom-lib");
        let boundary = classify_by_name(&dep).expect("should match encrypt pattern");
        assert_eq!(boundary.direction, CryptoDirection::Encrypt);
        assert_eq!(boundary.confidence, Confidence::Medium);
        assert!(boundary.output.is_none());
        assert!(boundary.param_roles.is_empty());
    }

    #[test]
    fn classify_by_name_strong_pattern_decrypt() {
        let dep = make_dep("decipherToken", "auth-utils");
        let boundary = classify_by_name(&dep).expect("should match decipher pattern");
        assert_eq!(boundary.direction, CryptoDirection::Decrypt);
        assert_eq!(boundary.confidence, Confidence::Medium);
    }

    #[test]
    fn classify_by_name_case_insensitive() {
        let dep = make_dep("AES_Encrypt", "some-lib");
        let boundary = classify_by_name(&dep).expect("should match case-insensitively");
        assert_eq!(boundary.direction, CryptoDirection::Encrypt);
    }

    #[test]
    fn classify_by_name_no_match() {
        let dep = make_dep("processData", "my-utils");
        assert!(classify_by_name(&dep).is_none());
    }

    #[test]
    fn classify_by_name_ambiguous_needs_crypto_context() {
        // "open" without crypto context should NOT match.
        let dep_fs = make_dep("open", "fs");
        assert!(
            classify_by_name(&dep_fs).is_none(),
            "open in fs module should not match"
        );

        // "open" WITH crypto context should match.
        let dep_crypto = make_dep("open", "crypto/cipher");
        let boundary =
            classify_by_name(&dep_crypto).expect("open in crypto module should match");
        assert_eq!(boundary.direction, CryptoDirection::Decrypt);
        assert_eq!(boundary.confidence, Confidence::Low);
    }

    #[test]
    fn classify_by_name_seal_without_context() {
        let dep = make_dep("seal", "database");
        assert!(classify_by_name(&dep).is_none());
    }

    #[test]
    fn classify_by_name_seal_with_crypto_context() {
        let dep = make_dep("seal", "nacl/box");
        let boundary = classify_by_name(&dep).expect("seal in nacl module should match");
        assert_eq!(boundary.direction, CryptoDirection::Encrypt);
        assert_eq!(boundary.confidence, Confidence::Low);
    }

    #[test]
    fn classify_by_name_unseal_strong_pattern() {
        // unseal is a strong pattern — no context needed.
        let dep = make_dep("unseal", "my-vault-lib");
        let boundary = classify_by_name(&dep).expect("unseal should match as strong pattern");
        assert_eq!(boundary.direction, CryptoDirection::Decrypt);
        assert_eq!(boundary.confidence, Confidence::Medium);
    }

    #[test]
    fn classify_all_prefers_layer1_over_layer2() {
        let registry = CryptoRegistry::load().unwrap();
        let dep = ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: "createDecipheriv".to_string(),
            source_module: "crypto".to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![5],
        };
        let boundaries = registry.classify_all_dependencies(&[dep], "typescript");
        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].confidence, Confidence::High);
        assert!(
            boundaries[0].output.is_some(),
            "Layer 1 results should have output semantics"
        );
    }

    #[test]
    fn classify_all_falls_back_to_layer2() {
        let registry = CryptoRegistry::load().unwrap();
        let dep = ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: "encryptPayload".to_string(),
            source_module: "my-custom-lib".to_string(),
            return_type: crate::types::TypeInfo::Unknown,
            param_types: vec![],
            call_sites: vec![20],
        };
        let boundaries = registry.classify_all_dependencies(&[dep], "typescript");
        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].confidence, Confidence::Medium);
        assert!(
            boundaries[0].output.is_none(),
            "Layer 2 results should not have output semantics"
        );
    }

    // -- Layer 3: entropy confirmation tests --

    #[test]
    fn confirm_entropy_upgrades_medium_to_high() {
        let boundary = CryptoBoundary {
            symbol: "encryptData".into(),
            source_module: "my-lib".into(),
            direction: CryptoDirection::Encrypt,
            output: None,
            confidence: Confidence::Medium,
            param_roles: HashMap::new(),
            call_sites: vec![10],
            input_entropy: None,
            output_entropy: None,
        };
        // Large entropy increase confirms encryption.
        let confirmed = confirm_with_entropy(&boundary, 2.0, 7.8);
        assert_eq!(confirmed.confidence, Confidence::High);
        assert_eq!(confirmed.input_entropy, Some(2.0));
        assert_eq!(confirmed.output_entropy, Some(7.8));
    }

    #[test]
    fn confirm_entropy_upgrades_low_to_medium() {
        let boundary = CryptoBoundary {
            symbol: "open".into(),
            source_module: "crypto/box".into(),
            direction: CryptoDirection::Decrypt,
            output: None,
            confidence: Confidence::Low,
            param_roles: HashMap::new(),
            call_sites: vec![5],
            input_entropy: None,
            output_entropy: None,
        };
        // Large entropy decrease confirms decryption.
        let confirmed = confirm_with_entropy(&boundary, 7.5, 3.0);
        assert_eq!(confirmed.confidence, Confidence::Medium);
    }

    #[test]
    fn confirm_entropy_downgrades_medium_to_low() {
        let boundary = CryptoBoundary {
            symbol: "encryptData".into(),
            source_module: "my-lib".into(),
            direction: CryptoDirection::Encrypt,
            output: None,
            confidence: Confidence::Medium,
            param_roles: HashMap::new(),
            call_sites: vec![10],
            input_entropy: None,
            output_entropy: None,
        };
        // No significant entropy delta → contradicts.
        let confirmed = confirm_with_entropy(&boundary, 5.0, 5.5);
        assert_eq!(confirmed.confidence, Confidence::Low);
    }

    #[test]
    fn confirm_entropy_does_not_downgrade_high() {
        let boundary = CryptoBoundary {
            symbol: "createDecipheriv".into(),
            source_module: "crypto".into(),
            direction: CryptoDirection::Decrypt,
            output: Some(OutputSemantics::Plaintext),
            confidence: Confidence::High,
            param_roles: HashMap::new(),
            call_sites: vec![1],
            input_entropy: None,
            output_entropy: None,
        };
        // No entropy delta, but High (Layer 1) stays High.
        let confirmed = confirm_with_entropy(&boundary, 5.0, 5.0);
        assert_eq!(confirmed.confidence, Confidence::High);
        assert_eq!(confirmed.input_entropy, Some(5.0));
    }

    #[test]
    fn confirm_entropy_low_stays_low_on_contradiction() {
        let boundary = CryptoBoundary {
            symbol: "seal".into(),
            source_module: "nacl".into(),
            direction: CryptoDirection::Encrypt,
            output: None,
            confidence: Confidence::Low,
            param_roles: HashMap::new(),
            call_sites: vec![3],
            input_entropy: None,
            output_entropy: None,
        };
        let confirmed = confirm_with_entropy(&boundary, 4.0, 4.5);
        assert_eq!(confirmed.confidence, Confidence::Low);
    }

    // -- Integration: all 3 layers --

    #[test]
    fn integration_all_layers() {
        let registry = CryptoRegistry::load().unwrap();
        let deps = vec![
            // Layer 1: known crypto API → High.
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "createDecipheriv".to_string(),
                source_module: "crypto".to_string(),
                return_type: crate::types::TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![5],
            },
            // Layer 2: naming heuristic → Medium.
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "decryptPayload".to_string(),
                source_module: "my-custom-lib".to_string(),
                return_type: crate::types::TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![15],
            },
            // Non-crypto dependency → no match.
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "readFile".to_string(),
                source_module: "fs".to_string(),
                return_type: crate::types::TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![20],
            },
        ];

        let boundaries = registry.classify_all_dependencies(&deps, "typescript");
        assert_eq!(boundaries.len(), 2);

        // Layer 1 result.
        let layer1 = boundaries.iter().find(|b| b.symbol == "createDecipheriv").unwrap();
        assert_eq!(layer1.confidence, Confidence::High);
        assert_eq!(layer1.output, Some(OutputSemantics::Plaintext));

        // Layer 2 result.
        let layer2 = boundaries.iter().find(|b| b.symbol == "decryptPayload").unwrap();
        assert_eq!(layer2.confidence, Confidence::Medium);
        assert!(layer2.output.is_none());

        // Layer 3: entropy confirmation upgrades Layer 2 result.
        let upgraded = confirm_with_entropy(layer2, 7.8, 3.0);
        assert_eq!(upgraded.confidence, Confidence::High);
    }

    // -- CryptoDetectionSummary tests --

    #[test]
    fn summary_from_empty_boundaries() {
        let summary = CryptoDetectionSummary::from_boundaries(vec![]);
        assert!(summary.boundaries.is_empty());
        assert!(summary.layers_used.is_empty());
        assert_eq!(summary.high_confidence_count, 0);
    }

    #[test]
    fn summary_tracks_layers_and_counts() {
        let boundaries = vec![
            CryptoBoundary {
                symbol: "createDecipheriv".into(),
                source_module: "crypto".into(),
                direction: CryptoDirection::Decrypt,
                output: Some(OutputSemantics::Plaintext),
                confidence: Confidence::High,
                param_roles: HashMap::new(),
                call_sites: vec![1],
                input_entropy: None,
                output_entropy: None,
            },
            CryptoBoundary {
                symbol: "encryptData".into(),
                source_module: "my-lib".into(),
                direction: CryptoDirection::Encrypt,
                output: None,
                confidence: Confidence::Medium,
                param_roles: HashMap::new(),
                call_sites: vec![10],
                input_entropy: None,
                output_entropy: None,
            },
        ];
        let summary = CryptoDetectionSummary::from_boundaries(boundaries);
        assert_eq!(summary.boundaries.len(), 2);
        assert_eq!(summary.layers_used, vec![1, 2]);
        assert_eq!(summary.high_confidence_count, 1);
    }

    #[test]
    fn summary_single_layer() {
        let boundaries = vec![CryptoBoundary {
            symbol: "decryptPayload".into(),
            source_module: "utils".into(),
            direction: CryptoDirection::Decrypt,
            output: None,
            confidence: Confidence::Medium,
            param_roles: HashMap::new(),
            call_sites: vec![5],
            input_entropy: None,
            output_entropy: None,
        }];
        let summary = CryptoDetectionSummary::from_boundaries(boundaries);
        assert_eq!(summary.layers_used, vec![2]);
        assert_eq!(summary.high_confidence_count, 0);
    }

    // -- Property tests --

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn arb_dependency() -> impl Strategy<Value = ExternalDependency> {
            (
                prop::sample::select(vec![
                    DependencyKind::FunctionCall,
                    DependencyKind::MethodCall,
                ]),
                "[a-zA-Z_][a-zA-Z0-9_.]{0,30}",
                "[a-zA-Z_/][a-zA-Z0-9_/.@-]{0,30}",
                proptest::collection::vec(any::<u32>(), 0..5),
            )
                .prop_map(|(kind, symbol, source_module, call_sites)| ExternalDependency {
                    kind,
                    symbol,
                    source_module,
                    return_type: crate::types::TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites,
                })
        }

        proptest! {
            #[test]
            fn classify_all_never_panics(
                deps in proptest::collection::vec(arb_dependency(), 0..20),
                language in prop::sample::select(vec!["typescript", "go", "rust"]),
            ) {
                let registry = CryptoRegistry::load().expect("registry should load");
                let boundaries = registry.classify_all_dependencies(&deps, language);
                // Every boundary must have a valid confidence.
                for b in &boundaries {
                    prop_assert!(
                        matches!(b.confidence, Confidence::High | Confidence::Medium | Confidence::Low)
                    );
                }
            }

            #[test]
            fn confirm_entropy_preserves_symbol(
                input_e in 0.0f64..=8.0,
                output_e in 0.0f64..=8.0,
            ) {
                let boundary = CryptoBoundary {
                    symbol: "testSymbol".into(),
                    source_module: "test-mod".into(),
                    direction: CryptoDirection::Encrypt,
                    output: None,
                    confidence: Confidence::Medium,
                    param_roles: HashMap::new(),
                    call_sites: vec![1],
                    input_entropy: None,
                    output_entropy: None,
                };
                let confirmed = confirm_with_entropy(&boundary, input_e, output_e);
                prop_assert_eq!(&confirmed.symbol, "testSymbol");
                prop_assert_eq!(&confirmed.source_module, "test-mod");
                prop_assert_eq!(confirmed.input_entropy, Some(input_e));
                prop_assert_eq!(confirmed.output_entropy, Some(output_e));
            }

            #[test]
            fn summary_high_count_matches_boundaries(
                n_high in 0usize..5,
                n_medium in 0usize..5,
            ) {
                let mut boundaries = Vec::new();
                for i in 0..n_high {
                    boundaries.push(CryptoBoundary {
                        symbol: format!("high_{i}"),
                        source_module: "crypto".into(),
                        direction: CryptoDirection::Encrypt,
                        output: Some(OutputSemantics::Ciphertext),
                        confidence: Confidence::High,
                        param_roles: HashMap::new(),
                        call_sites: vec![],
                        input_entropy: None,
                        output_entropy: None,
                    });
                }
                for i in 0..n_medium {
                    boundaries.push(CryptoBoundary {
                        symbol: format!("medium_{i}"),
                        source_module: "lib".into(),
                        direction: CryptoDirection::Decrypt,
                        output: None,
                        confidence: Confidence::Medium,
                        param_roles: HashMap::new(),
                        call_sites: vec![],
                        input_entropy: None,
                        output_entropy: None,
                    });
                }
                let summary = CryptoDetectionSummary::from_boundaries(boundaries);
                prop_assert_eq!(summary.high_confidence_count, n_high);
                prop_assert_eq!(summary.boundaries.len(), n_high + n_medium);
            }
        }
    }
}
