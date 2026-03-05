use extism_pdk::*;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize, Default)]
struct Recipe {
    category: Option<String>,
    index: Option<usize>,
}

struct Payload {
    category: &'static str,
    value: &'static str,
}

/// Dynamically build extreme-length strings to keep the .wasm binary small.
fn extreme_length_string(len: usize) -> String {
    "A".repeat(len)
}

/// All adversarial payloads, organized by category.
/// Extreme-length entries use sentinel values replaced at runtime.
const PAYLOADS: &[Payload] = &[
    // ── sql_injection (10) ──
    Payload { category: "sql_injection", value: "' OR '1'='1" },
    Payload { category: "sql_injection", value: "'; DROP TABLE users; --" },
    Payload { category: "sql_injection", value: "' UNION SELECT NULL, NULL, NULL --" },
    Payload { category: "sql_injection", value: "1' AND 1=1 --" },
    Payload { category: "sql_injection", value: "1' AND SLEEP(5) --" },
    Payload { category: "sql_injection", value: "admin'--" },
    Payload { category: "sql_injection", value: "' OR 1=1 LIMIT 1 --" },
    Payload { category: "sql_injection", value: "1; EXEC xp_cmdshell('whoami') --" },
    Payload { category: "sql_injection", value: "' UNION SELECT username, password FROM users --" },
    Payload { category: "sql_injection", value: "')) OR (('1'='1" },
    // ── xss (10) ──
    Payload { category: "xss", value: "<script>alert(1)</script>" },
    Payload { category: "xss", value: "<img src=x onerror=alert(1)>" },
    Payload { category: "xss", value: "<svg onload=alert(1)>" },
    Payload { category: "xss", value: "javascript:alert(1)" },
    Payload { category: "xss", value: "<body onload=alert(1)>" },
    Payload { category: "xss", value: "\"><script>alert(document.cookie)</script>" },
    Payload { category: "xss", value: "'-alert(1)-'" },
    Payload { category: "xss", value: "<iframe src=\"javascript:alert(1)\">" },
    Payload { category: "xss", value: "<details open ontoggle=alert(1)>" },
    Payload { category: "xss", value: "{{constructor.constructor('return alert(1)')()}}" },
    // ── unicode_edge (10) ──
    Payload { category: "unicode_edge", value: "\u{FEFF}test" },
    Payload { category: "unicode_edge", value: "test\u{200B}test" },
    Payload { category: "unicode_edge", value: "\u{202E}gnirts desreveR" },
    Payload { category: "unicode_edge", value: "te\u{0308}st" },
    Payload { category: "unicode_edge", value: "\u{0000}" },
    Payload { category: "unicode_edge", value: "\u{FFFD}" },
    Payload { category: "unicode_edge", value: "\u{1F4A9}" },
    Payload { category: "unicode_edge", value: "a\u{0300}\u{0301}\u{0302}\u{0303}\u{0304}" },
    Payload { category: "unicode_edge", value: "\u{200D}" },
    Payload { category: "unicode_edge", value: "\u{2028}line\u{2029}separator" },
    // ── null_bytes (8) ──
    Payload { category: "null_bytes", value: "\x00" },
    Payload { category: "null_bytes", value: "test\x00hidden" },
    Payload { category: "null_bytes", value: "%00" },
    Payload { category: "null_bytes", value: "test%00.txt" },
    Payload { category: "null_bytes", value: "\x00\x00\x00" },
    Payload { category: "null_bytes", value: "admin\x00ignored" },
    Payload { category: "null_bytes", value: "../etc/passwd\x00.png" },
    Payload { category: "null_bytes", value: "test\x00\x0a\x0d" },
    // ── extreme_length (6) — sentinel "EXTREME:<len>" replaced at runtime ──
    Payload { category: "extreme_length", value: "" },
    Payload { category: "extreme_length", value: "x" },
    Payload { category: "extreme_length", value: "EXTREME:256" },
    Payload { category: "extreme_length", value: "EXTREME:1000" },
    Payload { category: "extreme_length", value: "EXTREME:10000" },
    Payload { category: "extreme_length", value: "EXTREME:100000" },
    // ── format_string (8) ──
    Payload { category: "format_string", value: "%s%s%s%s%n" },
    Payload { category: "format_string", value: "${7*7}" },
    Payload { category: "format_string", value: "{{7*7}}" },
    Payload { category: "format_string", value: "${constructor.constructor('return 1')()}" },
    Payload { category: "format_string", value: "#{7*7}" },
    Payload { category: "format_string", value: "<%= 7*7 %>" },
    Payload { category: "format_string", value: "${`id`}" },
    Payload { category: "format_string", value: "{{config.__class__.__init__.__globals__}}" },
    // ── path_traversal (10) ──
    Payload { category: "path_traversal", value: "../../../etc/passwd" },
    Payload { category: "path_traversal", value: "..\\..\\..\\windows\\system32\\config\\sam" },
    Payload { category: "path_traversal", value: "....//....//....//etc/passwd" },
    Payload { category: "path_traversal", value: "%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd" },
    Payload { category: "path_traversal", value: "..%252f..%252f..%252fetc/passwd" },
    Payload { category: "path_traversal", value: "/etc/passwd" },
    Payload { category: "path_traversal", value: "....//etc/passwd" },
    Payload { category: "path_traversal", value: "..%c0%af..%c0%afetc/passwd" },
    Payload { category: "path_traversal", value: "file:///etc/passwd" },
    Payload { category: "path_traversal", value: "/proc/self/environ" },
    // ── encoding (8) ──
    Payload { category: "encoding", value: "%3Cscript%3Ealert(1)%3C%2Fscript%3E" },
    Payload { category: "encoding", value: "&#60;script&#62;alert(1)&#60;/script&#62;" },
    Payload { category: "encoding", value: "\\u003cscript\\u003ealert(1)\\u003c/script\\u003e" },
    Payload { category: "encoding", value: "PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==" },
    Payload { category: "encoding", value: "%c0%bcscript%c0%bealert(1)%c0%bc/script%c0%be" },
    Payload { category: "encoding", value: "\\x3cscript\\x3ealert(1)\\x3c/script\\x3e" },
    Payload { category: "encoding", value: "data:text/html,<script>alert(1)</script>" },
    Payload { category: "encoding", value: "&#x3C;script&#x3E;alert(1)&#x3C;/script&#x3E;" },
];

/// Resolve the actual string value for a payload, expanding extreme-length sentinels.
fn resolve_value(payload: &Payload) -> String {
    if let Some(len_str) = payload.value.strip_prefix("EXTREME:") {
        let len: usize = len_str.parse().unwrap_or(256);
        extreme_length_string(len)
    } else {
        payload.value.to_string()
    }
}

/// Filter payloads by category, returning indices into PAYLOADS.
fn indices_for_category(category: &str) -> Vec<usize> {
    PAYLOADS
        .iter()
        .enumerate()
        .filter(|(_, p)| p.category == category)
        .map(|(i, _)| i)
        .collect()
}

/// Core generation logic shared by both exported functions.
fn generate(input: String) -> FnResult<String> {
    let recipe: Recipe = if input.is_empty() {
        Recipe::default()
    } else {
        serde_json::from_str(&input).unwrap_or_default()
    };

    let idx = match (&recipe.category, recipe.index) {
        (Some(cat), Some(local_idx)) => {
            // Specific category + index: select that exact payload within category
            let cat_indices = indices_for_category(cat);
            if cat_indices.is_empty() {
                0
            } else {
                cat_indices[local_idx % cat_indices.len()]
            }
        }
        (Some(cat), None) => {
            // Category filter, round-robin within that category
            let cat_indices = indices_for_category(cat);
            if cat_indices.is_empty() {
                0
            } else {
                let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
                cat_indices[counter as usize % cat_indices.len()]
            }
        }
        (None, Some(global_idx)) => {
            // Direct global index
            global_idx % PAYLOADS.len()
        }
        (None, None) => {
            // No filter: round-robin through all payloads
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
            counter as usize % PAYLOADS.len()
        }
    };

    let payload = &PAYLOADS[idx];
    let value = resolve_value(payload);

    // Compute local index within category for the recipe
    let local_idx = indices_for_category(payload.category)
        .iter()
        .position(|&i| i == idx)
        .unwrap_or(0);

    let result = serde_json::json!({
        "id": format!("adversarial-{}-{}", payload.category, local_idx),
        "value": value,
        "recipe": {
            "category": payload.category,
            "index": local_idx
        }
    });
    Ok(serde_json::to_string(&result)?)
}

/// Exported as `adversarial` for param_generators config entries.
#[plugin_fn]
pub fn adversarial(input: String) -> FnResult<String> {
    generate(input)
}

/// Exported as `AdversarialString` for type-name generators config entries.
#[plugin_fn]
pub fn AdversarialString(input: String) -> FnResult<String> {
    generate(input)
}
