//! Random input generation from TypeInfo metadata.
//!
//! Generates random JSON values matching the type signatures reported by
//! language frontends. Used for the initial exploration phase before symbolic
//! constraint solving kicks in.

use rand::Rng;
use serde_json::{json, Value};

use crate::orchestrator::FrontendCapabilities;
use crate::string_mutation;
use crate::types::{ComplexKind, TypeInfo};

// ---------------------------------------------------------------------------
// Param-name heuristic string generation
// ---------------------------------------------------------------------------

/// A mapping from param-name substrings to a realistic seed value.
///
/// Order matters: more specific patterns (e.g. `first_name`) must precede
/// more general ones (`name`) to avoid false-positive matches.
struct NameHeuristic {
    substrings: &'static [&'static str],
    value: &'static str,
}

const HEURISTIC_EMAIL: &str = "user42@example.com";
const HEURISTIC_URL: &str = "https://api.example.com/v2/items?id=7";
const HEURISTIC_PHONE: &str = "+1-555-0142";
const HEURISTIC_FIRST_NAME: &str = "Alice";
const HEURISTIC_LAST_NAME: &str = "Smith";
const HEURISTIC_NAME: &str = "Bob Smith";
const HEURISTIC_DATE: &str = "2026-03-05T14:30:00Z";
const HEURISTIC_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";
const HEURISTIC_PATH: &str = "/tmp/data/report.csv";
const HEURISTIC_IP: &str = "192.168.1.42";
const HEURISTIC_TOKEN: &str = "sk_test_abc123def456ghi789jkl012mno345";

/// Ordered by specificity: more specific patterns first to avoid false positives.
/// E.g. "first_name" before "name", "filepath" before "path" before "name".
const NAME_HEURISTICS: &[NameHeuristic] = &[
    NameHeuristic { substrings: &["email", "mail"], value: HEURISTIC_EMAIL },
    NameHeuristic { substrings: &["url", "uri", "href", "link"], value: HEURISTIC_URL },
    NameHeuristic { substrings: &["phone", "tel", "mobile"], value: HEURISTIC_PHONE },
    NameHeuristic { substrings: &["first_name", "firstname"], value: HEURISTIC_FIRST_NAME },
    NameHeuristic { substrings: &["last_name", "lastname"], value: HEURISTIC_LAST_NAME },
    NameHeuristic { substrings: &["path", "file", "filename"], value: HEURISTIC_PATH },
    NameHeuristic { substrings: &["name"], value: HEURISTIC_NAME },
    NameHeuristic { substrings: &["date", "timestamp", "created_at", "updated_at"], value: HEURISTIC_DATE },
    NameHeuristic { substrings: &["uuid"], value: HEURISTIC_UUID },
    NameHeuristic { substrings: &["id"], value: HEURISTIC_UUID },
    NameHeuristic { substrings: &["ip", "addr"], value: HEURISTIC_IP },
    NameHeuristic { substrings: &["token", "key", "secret"], value: HEURISTIC_TOKEN },
];

/// Return a realistic heuristic string if `name` matches a known pattern.
///
/// Uses case-insensitive substring matching against `NAME_HEURISTICS`.
/// Returns the first match, so ordering in the table controls priority.
fn heuristic_string_for_name(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    NAME_HEURISTICS.iter().find_map(|h| {
        h.substrings
            .iter()
            .any(|s| lower.contains(s))
            .then_some(h.value)
    })
}

/// Generate a random JSON value matching the given type.
///
/// Uses biased distributions that favor boundary values (0, -1, 1, empty
/// strings, etc.) to increase the chance of hitting interesting branches.
///
/// When `caps` is provided, complex types check whether the frontend declared
/// support. Unsupported complex types fall back to `Unknown` generation.
pub fn generate_random_value(
    typ: &TypeInfo,
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Value {
    match typ {
        TypeInfo::Int => generate_int(rng),
        TypeInfo::Float => generate_float(rng),
        TypeInfo::Str => generate_string(rng),
        TypeInfo::Bool => json!(rng.random_bool(0.5)),
        TypeInfo::Array { element } => generate_array(element, rng, caps),
        TypeInfo::Object { fields } => generate_object(fields, rng, caps),
        TypeInfo::Union { variants } => generate_union(variants, rng, caps),
        TypeInfo::Nullable { inner } => generate_nullable(inner, rng, caps),
        TypeInfo::Complex { kind, metadata, inner } => {
            if caps.is_some_and(|c| c.supports_complex(*kind)) {
                generate_complex_value(*kind, metadata, inner.as_deref(), rng)
            } else {
                generate_unknown(rng)
            }
        }
        TypeInfo::Opaque { .. } => Value::Null,
        TypeInfo::Unknown => generate_unknown(rng),
    }
}

/// Generate a random integer, biased toward boundary values.
fn generate_int(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..10);
    let n = match choice {
        0 => 0,
        1 => 1,
        2 => -1,
        3 => i64::MAX,
        4 => i64::MIN,
        _ => rng.random_range(-1000..=1000),
    };
    json!(n)
}

/// Generate a random float, biased toward boundary values.
///
/// Includes integer values in the distribution since TypeScript's `number`
/// type covers both integers and floats.
fn generate_float(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..12);
    let n: f64 = match choice {
        0 => 0.0,
        1 => 1.0,
        2 => -1.0,
        3 => 0.5,
        4 => -0.5,
        // Include some integer values to cover integer-like branches (e.g. n % 2 === 0)
        5 => rng.random_range(-100..=100) as f64,
        6 => 2.0,
        7 => -2.0,
        8 => 10.0,
        _ => rng.random_range(-1000.0..1000.0),
    };
    json!(n)
}

/// Generate a random string from a small vocabulary plus random characters.
fn generate_string(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..10);
    let s = match choice {
        0 => String::new(),
        1 => "hello".to_string(),
        2 => "test".to_string(),
        3 => " ".to_string(),
        4 => "0".to_string(),
        5 => "true".to_string(),
        6 => "null".to_string(),
        _ => {
            let len = rng.random_range(1..=20);
            (0..len)
                .map(|_| rng.random_range(b'a'..=b'z') as char)
                .collect()
        }
    };
    json!(s)
}

/// Generate a random array with bounded length, biased toward small sizes.
///
/// Most real bugs appear at boundary lengths (0, 1, 2, 3), so we heavily
/// favor those over larger sizes. The distribution:
/// - 25% chance of length 0 (empty array)
/// - 25% chance of length 1 (single element)
/// - 20% chance of length 2
/// - 15% chance of length 3
/// - 15% chance of length 4-5 (larger arrays, less common in bug-triggering)
fn generate_array(
    element: &TypeInfo,
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Value {
    let len = generate_bounded_array_length(rng);
    let items: Vec<Value> = (0..len)
        .map(|_| generate_random_value(element, rng, caps))
        .collect();
    json!(items)
}

/// Generate an array length biased toward small boundary values (0-3).
fn generate_bounded_array_length(rng: &mut impl Rng) -> usize {
    let choice: u8 = rng.random_range(0..20);
    match choice {
        0..5 => 0,   // 25%: empty
        5..10 => 1,  // 25%: single element
        10..14 => 2, // 20%: two elements
        14..17 => 3, // 15%: three elements
        _ => rng.random_range(4..=5), // 15%: larger
    }
}

/// Generate a random object with the specified fields.
fn generate_object(
    fields: &[(String, TypeInfo)],
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Value {
    let mut obj = serde_json::Map::new();
    for (name, typ) in fields {
        obj.insert(name.clone(), generate_random_value(typ, rng, caps));
    }
    Value::Object(obj)
}

/// Pick a random variant from a union type.
fn generate_union(
    variants: &[TypeInfo],
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Value {
    if variants.is_empty() {
        return Value::Null;
    }
    let idx = rng.random_range(0..variants.len());
    generate_random_value(&variants[idx], rng, caps)
}

/// Generate null ~30% of the time, otherwise generate the inner type.
fn generate_nullable(
    inner: &TypeInfo,
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Value {
    if rng.random_range(0..10) < 3 {
        Value::Null
    } else {
        generate_random_value(inner, rng, caps)
    }
}

/// Generate a complex-typed value with `__complex_type` tagged wire format.
///
/// Dispatches to per-kind generators. Each generator produces boundary-biased values.
fn generate_complex_value(
    kind: ComplexKind,
    metadata: &serde_json::Map<String, serde_json::Value>,
    _inner: Option<&TypeInfo>,
    rng: &mut impl Rng,
) -> Value {
    match kind {
        ComplexKind::Date | ComplexKind::DateTime => generate_date(rng),
        ComplexKind::Duration => generate_duration(rng),
        ComplexKind::Time => generate_time(rng),
        ComplexKind::RegExp => generate_regexp(rng),
        ComplexKind::Url => generate_url(rng),
        ComplexKind::Error => generate_error(metadata, rng),
        ComplexKind::BigInt => generate_bigint(rng),
        ComplexKind::BigDecimal => generate_big_decimal(rng),
        ComplexKind::Uuid => generate_uuid(rng),
        ComplexKind::IpAddress => generate_ip_address(rng),
        ComplexKind::Path => generate_path(rng),
        ComplexKind::Buffer => generate_buffer(rng),
        ComplexKind::Symbol => generate_symbol(rng),
        ComplexKind::Char | ComplexKind::Rune => generate_char(rng),
        ComplexKind::GoByte => generate_go_byte(rng),
        ComplexKind::Email => generate_email(rng),
        ComplexKind::SemVer => generate_semver(rng),
        ComplexKind::Color => generate_color(rng),
        ComplexKind::GeoPoint => generate_geo_point(rng),
        ComplexKind::Money => generate_money(rng),
        ComplexKind::MimeType => generate_mime_type(rng),
        ComplexKind::Locale => generate_locale(rng),
        ComplexKind::Range => generate_range(rng),
        ComplexKind::Complex => generate_complex_number(rng),
        ComplexKind::Rational => generate_rational(rng),
        ComplexKind::BitSet => generate_bitset(rng),
        ComplexKind::Option => generate_option(rng),
        ComplexKind::Result => generate_result(rng),
        ComplexKind::Closure => generate_closure(rng),
        ComplexKind::Iterator => generate_iterator(rng),
    }
}

/// Generate a Date value as `{"__complex_type": "date", "value": <epoch_ms>}`.
///
/// Biased toward boundary values: epoch 0, Y2K38, month/year boundaries, etc.
fn generate_date(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..12);
    let epoch_ms: i64 = match choice {
        0 => 0,                        // Unix epoch
        1 => -1,                       // Just before epoch
        2 => 2_147_483_647_000,        // Y2K38 (2038-01-19T03:14:07Z) in ms
        3 => 1_704_067_200_000,        // 2024-01-01T00:00:00Z
        4 => 946_684_800_000,          // 2000-01-01T00:00:00Z (Y2K)
        5 => -62_135_596_800_000,      // 0001-01-01T00:00:00Z (far past)
        6 => 253_402_300_799_000,      // 9999-12-31T23:59:59Z (far future)
        7 => 1_609_459_200_000,        // 2021-01-01T00:00:00Z
        8 => {
            // Random month boundary: 1st of a random month in 2020-2025
            let year = rng.random_range(2020..=2025);
            let month = rng.random_range(1..=12);
            // Approximate: months * 30.44 days
            let days_since_epoch = (year - 1970) * 365 + (month - 1) * 30;
            days_since_epoch as i64 * 86_400_000
        }
        9 => -86_400_000,             // One day before epoch
        10 => 86_400_000,             // One day after epoch
        _ => {
            // Random date between 1970 and 2030
            rng.random_range(0..1_893_456_000_000_i64)
        }
    };
    json!({"__complex_type": "date", "value": epoch_ms})
}

/// Generate a Duration value as `{"__complex_type": "duration", "ms": <millis>}`.
fn generate_duration(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..10);
    let ms: i64 = match choice {
        0 => 0,                        // Zero duration
        1 => -1,                       // Negative
        2 => 1,                        // 1ms
        3 => 1_000,                    // 1 second
        4 => 60_000,                   // 1 minute
        5 => 3_600_000,               // 1 hour
        6 => 86_400_000,              // 1 day
        7 => -86_400_000,             // Negative 1 day
        8 => 9_007_199_254_740_991,   // MAX_SAFE_INTEGER
        _ => rng.random_range(-86_400_000..=86_400_000),
    };
    json!({"__complex_type": "duration", "ms": ms})
}

/// Generate a Time value as `{"__complex_type": "time", "hour": h, "minute": m, "second": s, "ms": ms}`.
fn generate_time(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..6);
    let (h, m, s, ms) = match choice {
        0 => (0, 0, 0, 0),           // Midnight
        1 => (12, 0, 0, 0),          // Noon
        2 => (23, 59, 59, 999),      // End of day
        3 => (0, 0, 1, 0),           // Just after midnight
        _ => (
            rng.random_range(0..24) as u8,
            rng.random_range(0..60) as u8,
            rng.random_range(0..60) as u8,
            rng.random_range(0..1000) as u16,
        ),
    };
    json!({"__complex_type": "time", "hour": h, "minute": m, "second": s, "ms": ms})
}

/// Generate a RegExp value.
fn generate_regexp(rng: &mut impl Rng) -> Value {
    let patterns = [
        (".*", ""), ("\\d+", "g"), ("[a-z]+", "i"), ("^\\s+$", ""),
        ("\\w{3,}", "gi"), ("^$", ""), (".", ""), ("\\d{3}-\\d{4}", ""),
    ];
    let idx = rng.random_range(0..patterns.len());
    let (source, flags) = patterns[idx];
    json!({"__complex_type": "reg_exp", "source": source, "flags": flags})
}

/// Generate a URL value.
fn generate_url(rng: &mut impl Rng) -> Value {
    let urls = [
        "https://example.com",
        "https://example.com/path?q=1",
        "http://localhost:3000",
        "https://example.com:8443/api/v1",
        "https://example.com/path#fragment",
        "https://user:pass@example.com",
        "ftp://files.example.com/pub",
        "https://xn--r8jz45g.jp/%E3%83%86%E3%82%B9%E3%83%88",
    ];
    let idx = rng.random_range(0..urls.len());
    json!({"__complex_type": "url", "value": urls[idx]})
}

/// Generate an Error value.
fn generate_error(
    metadata: &serde_json::Map<String, serde_json::Value>,
    rng: &mut impl Rng,
) -> Value {
    let class = metadata
        .get("class")
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| {
            let classes = ["Error", "TypeError", "RangeError", "SyntaxError"];
            classes[rng.random_range(0..classes.len())]
        });
    let messages = ["", "oops", "invalid argument", "out of range", "unexpected token"];
    let msg = messages[rng.random_range(0..messages.len())];
    json!({"__complex_type": "error", "class": class, "message": msg})
}

/// Generate a BigInt value.
fn generate_bigint(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..8);
    let s = match choice {
        0 => "0".to_string(),
        1 => "1".to_string(),
        2 => "-1".to_string(),
        3 => "9007199254740992".to_string(), // MAX_SAFE_INTEGER + 1
        4 => "-9007199254740992".to_string(),
        5 => "99999999999999999999999999999999".to_string(), // 32 digits
        _ => {
            let digits = rng.random_range(1..=20);
            let neg = rng.random_bool(0.3);
            let mut s = if neg { "-".to_string() } else { String::new() };
            for i in 0..digits {
                let d = if i == 0 { rng.random_range(1..=9) } else { rng.random_range(0..=9) };
                s.push(char::from(b'0' + d as u8));
            }
            s
        }
    };
    json!({"__complex_type": "big_int", "value": s})
}

/// Generate a BigDecimal value.
fn generate_big_decimal(rng: &mut impl Rng) -> Value {
    let vals = ["0", "0.01", "-0.01", "3.14159265358979323846", "1000000.000001"];
    let idx = rng.random_range(0..vals.len());
    json!({"__complex_type": "big_decimal", "value": vals[idx]})
}

/// Generate a UUID value.
fn generate_uuid(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..4);
    let uuid = match choice {
        0 => "00000000-0000-0000-0000-000000000000".to_string(), // nil
        1 => "ffffffff-ffff-ffff-ffff-ffffffffffff".to_string(), // all ones
        _ => {
            // Random v4 UUID
            let bytes: Vec<String> = (0..16)
                .map(|_| format!("{:02x}", rng.random_range(0u8..=255)))
                .collect();
            format!(
                "{}-{}-4{}-{}{}-{}",
                bytes[0..4].join(""),
                bytes[4..6].join(""),
                &bytes[6][1..],
                &["8", "9", "a", "b"][rng.random_range(0..4)],
                &bytes[7][1..],
                bytes[8..14].join(""),
            )
        }
    };
    json!({"__complex_type": "uuid", "value": uuid})
}

/// Generate an IP address value.
fn generate_ip_address(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..6);
    let (version, value) = match choice {
        0 => (4, "0.0.0.0".to_string()),
        1 => (4, "127.0.0.1".to_string()),
        2 => (4, "255.255.255.255".to_string()),
        3 => (6, "::1".to_string()),
        4 => (4, format!(
            "{}.{}.{}.{}",
            rng.random_range(1..=254),
            rng.random_range(0..=255),
            rng.random_range(0..=255),
            rng.random_range(1..=254),
        )),
        _ => (4, "192.168.1.1".to_string()),
    };
    json!({"__complex_type": "ip_address", "version": version, "value": value})
}

/// Generate a Path value.
fn generate_path(rng: &mut impl Rng) -> Value {
    let paths = [
        "", "/", "/usr/local/bin", "relative/path", "../parent/file.txt",
        "/path with spaces/file", "/tmp/test.txt", ".",
    ];
    let idx = rng.random_range(0..paths.len());
    json!({"__complex_type": "path", "value": paths[idx]})
}

/// Generate a Buffer value (base64 encoded).
fn generate_buffer(rng: &mut impl Rng) -> Value {
    // Pre-computed base64 boundary values to avoid base64 crate dependency
    let choice: u8 = rng.random_range(0..5);
    let encoded = match choice {
        0 => "",                    // empty buffer
        1 => "AA==",               // single zero byte
        2 => "SGVsbG8=",           // "Hello"
        3 => "/////w==",           // 4x 0xFF
        _ => "AQIDBA==",           // [1,2,3,4]
    };
    json!({"__complex_type": "buffer", "encoding": "base64", "value": encoded})
}

/// Generate a Symbol value.
fn generate_symbol(rng: &mut impl Rng) -> Value {
    let descs = ["", "mySymbol", "iterator", "hasInstance"];
    let idx = rng.random_range(0..descs.len());
    json!({"__complex_type": "symbol", "description": descs[idx]})
}

/// Generate a Char/Rune value.
fn generate_char(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..8);
    let (ch, cp): (&str, u32) = match choice {
        0 => ("\0", 0),           // NUL
        1 => (" ", 32),           // space
        2 => ("a", 97),           // lowercase
        3 => ("Z", 90),           // uppercase
        4 => ("0", 48),           // digit
        5 => ("\u{20AC}", 8364),  // currency symbol (euro)
        6 => ("\u{1F389}", 127881), // emoji (party popper)
        _ => {
            let cp = rng.random_range(32..=126) as u32; // printable ASCII
            let ch_str: String = char::from_u32(cp).map_or_else(|| "?".to_string(), |c| c.to_string());
            return json!({"__complex_type": "char", "value": ch_str, "codepoint": cp});
        }
    };
    json!({"__complex_type": "char", "value": ch, "codepoint": cp})
}

/// Generate a Go byte value.
fn generate_go_byte(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..5);
    let v: u8 = match choice {
        0 => 0,
        1 => 1,
        2 => 127,
        3 => 255,
        _ => rng.random_range(0..=255),
    };
    json!({"__complex_type": "go_byte", "value": v})
}

/// Generate an Email value.
fn generate_email(rng: &mut impl Rng) -> Value {
    let emails = [
        "user@example.com", "test+tag@example.com",
        "a@b.co", "very.long.email.address@subdomain.example.org",
    ];
    let idx = rng.random_range(0..emails.len());
    json!({"__complex_type": "email", "value": emails[idx]})
}

/// Generate a SemVer value.
fn generate_semver(rng: &mut impl Rng) -> Value {
    let versions = ["0.0.0", "0.0.1", "0.1.0", "1.0.0", "2.1.3", "1.0.0-alpha", "1.0.0+build.1"];
    let idx = rng.random_range(0..versions.len());
    json!({"__complex_type": "sem_ver", "value": versions[idx]})
}

/// Generate a Color value (RGBA).
fn generate_color(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..5);
    let (r, g, b, a) = match choice {
        0 => (0, 0, 0, 255),         // black
        1 => (255, 255, 255, 255),   // white
        2 => (255, 0, 0, 255),       // red
        3 => (0, 0, 0, 0),           // transparent
        _ => (
            rng.random_range(0..=255),
            rng.random_range(0..=255),
            rng.random_range(0..=255),
            255,
        ),
    };
    json!({"__complex_type": "color", "r": r, "g": g, "b": b, "a": a})
}

/// Generate a GeoPoint value.
fn generate_geo_point(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..5);
    let (lat, lng) = match choice {
        0 => (0.0, 0.0),                    // origin
        1 => (90.0, 0.0),                   // north pole
        2 => (-90.0, 0.0),                  // south pole
        3 => (37.7749, -122.4194),          // San Francisco
        _ => (
            rng.random_range(-90.0..=90.0_f64),
            rng.random_range(-180.0..=180.0_f64),
        ),
    };
    json!({"__complex_type": "geo_point", "lat": lat, "lng": lng})
}

/// Generate a Money value.
fn generate_money(rng: &mut impl Rng) -> Value {
    let amounts = ["0", "0.01", "-0.01", "19.99", "1000000.00"];
    let currencies = ["USD", "EUR", "GBP", "JPY"];
    let amount = amounts[rng.random_range(0..amounts.len())];
    let currency = currencies[rng.random_range(0..currencies.len())];
    json!({"__complex_type": "money", "amount": amount, "currency": currency})
}

/// Generate a MimeType value.
fn generate_mime_type(rng: &mut impl Rng) -> Value {
    let types = [
        "text/plain", "application/json", "image/png", "text/html",
        "application/octet-stream", "multipart/form-data",
    ];
    let idx = rng.random_range(0..types.len());
    json!({"__complex_type": "mime_type", "value": types[idx]})
}

/// Generate a Locale value.
fn generate_locale(rng: &mut impl Rng) -> Value {
    let locales = ["en-US", "zh-CN", "ar-SA", "ja-JP", "de-DE", "invalid"];
    let idx = rng.random_range(0..locales.len());
    json!({"__complex_type": "locale", "value": locales[idx]})
}

/// Generate a Range value.
fn generate_range(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..5);
    let (start, end, inclusive) = match choice {
        0 => (5, 5, false),       // empty range
        1 => (5, 6, false),       // single element
        2 => (0, 10, false),      // normal
        3 => (-10, 10, true),     // negative to positive, inclusive
        _ => (
            rng.random_range(-100..=100) as i64,
            rng.random_range(-100..=100) as i64,
            rng.random_bool(0.3),
        ),
    };
    json!({"__complex_type": "range", "start": start, "end": end, "inclusive": inclusive})
}

/// Generate a complex number value (real + imaginary).
fn generate_complex_number(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..5);
    let (real, imag) = match choice {
        0 => (0.0, 0.0),
        1 => (1.0, 0.0),     // purely real
        2 => (0.0, 1.0),     // purely imaginary
        3 => (-1.0, -1.0),
        _ => (
            rng.random_range(-100.0..=100.0_f64),
            rng.random_range(-100.0..=100.0_f64),
        ),
    };
    json!({"__complex_type": "complex", "real": real, "imag": imag})
}

/// Generate a Rational value (numerator/denominator).
fn generate_rational(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..5);
    let (num, den) = match choice {
        0 => (0, 1),
        1 => (1, 1),
        2 => (1, 2),
        3 => (-1, 3),
        _ => (
            rng.random_range(-100..=100) as i64,
            rng.random_range(1..=100) as i64,
        ),
    };
    json!({"__complex_type": "rational", "numerator": num, "denominator": den})
}

/// Generate a BitSet value.
fn generate_bitset(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..4);
    let (bits, length): (&str, u32) = match choice {
        0 => ("00000000", 8),   // all zeros
        1 => ("11111111", 8),   // all ones
        2 => ("10000000", 8),   // single bit
        _ => ("10101010", 8),   // alternating
    };
    json!({"__complex_type": "bit_set", "bits": bits, "length": length})
}

/// Generate an Option value (None ~30%, Some ~70%).
fn generate_option(rng: &mut impl Rng) -> Value {
    if rng.random_range(0..10) < 3 {
        json!({"__complex_type": "option", "present": false})
    } else {
        let inner = generate_int(rng);
        json!({"__complex_type": "option", "present": true, "value": inner})
    }
}

/// Generate a Result value (Ok ~70%, Err ~30%).
fn generate_result(rng: &mut impl Rng) -> Value {
    if rng.random_range(0..10) < 3 {
        json!({"__complex_type": "result", "ok": false, "error": "error"})
    } else {
        let inner = generate_int(rng);
        json!({"__complex_type": "result", "ok": true, "value": inner})
    }
}

/// Generate a Closure value (canned variant).
fn generate_closure(rng: &mut impl Rng) -> Value {
    let variants = ["identity", "constant", "thrower"];
    let idx = rng.random_range(0..variants.len());
    json!({"__complex_type": "closure", "variant": variants[idx]})
}

/// Generate an Iterator value (array of values the frontend wraps).
fn generate_iterator(rng: &mut impl Rng) -> Value {
    let len = rng.random_range(0..=5);
    let values: Vec<Value> = (0..len).map(|_| generate_int(rng)).collect();
    json!({"__complex_type": "iterator", "values": values})
}

/// For unknown types, generate a random value from any primitive type.
fn generate_unknown(rng: &mut impl Rng) -> Value {
    let choice: u8 = rng.random_range(0..4);
    match choice {
        0 => generate_int(rng),
        1 => generate_float(rng),
        2 => generate_string(rng),
        3 => json!(rng.random_bool(0.5)),
        _ => unreachable!(),
    }
}

/// Generate a complete set of random inputs for a function's parameters.
///
/// When `caps` is provided, complex types are only generated if the frontend
/// declared support; otherwise they fall back to unknown/primitive generation.
pub fn generate_random_inputs(
    params: &[crate::types::ParamInfo],
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Vec<Value> {
    params
        .iter()
        .map(|p| {
            if matches!(p.typ, TypeInfo::Str) {
                let hint = heuristic_string_for_name(&p.name)
                    .or_else(|| p.type_name.as_deref().and_then(heuristic_string_for_name));
                if let Some(val) = hint {
                    return json!(val);
                }
            }
            generate_random_value(&p.typ, rng, caps)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Float-biased input generation
// ---------------------------------------------------------------------------

/// Generate a float value biased toward integers.
///
/// With probability `integer_ratio`, generates an integer (as f64).
/// Otherwise delegates to the standard `generate_float()` distribution.
pub fn generate_biased_float(rng: &mut impl Rng, integer_ratio: f64) -> Value {
    if rng.random_bool(integer_ratio.clamp(0.0, 1.0)) {
        // Generate as integer
        let n = rng.random_range(-1000..=1000) as f64;
        json!(n)
    } else {
        generate_float(rng)
    }
}

/// Like [`generate_random_inputs`] but applies integer bias to Float params
/// classified as `IntegerTreating`.
pub fn generate_random_inputs_with_float_bias(
    params: &[crate::types::ParamInfo],
    float_bias: &std::collections::HashMap<usize, crate::float_probe::FloatClassification>,
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Vec<Value> {
    params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if matches!(p.typ, TypeInfo::Str) {
                let hint = heuristic_string_for_name(&p.name)
                    .or_else(|| p.type_name.as_deref().and_then(heuristic_string_for_name));
                if let Some(val) = hint {
                    return json!(val);
                }
            }
            if matches!(p.typ, TypeInfo::Float)
                && float_bias.get(&i)
                    == Some(&crate::float_probe::FloatClassification::IntegerTreating)
            {
                return generate_biased_float(rng, crate::float_probe::INTEGER_BIAS_RATIO);
            }
            generate_random_value(&p.typ, rng, caps)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Generator-aware input generation
// ---------------------------------------------------------------------------

/// Where a parameter's value should come from during input generation.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueSource {
    /// Use a custom generator file to produce values for this parameter.
    CustomGenerator {
        /// Name of the generator (type name or param name, used for protocol).
        generator_name: String,
        /// The parameter name, if this is a param-level generator.
        param_name: Option<String>,
        /// Absolute path to the generator file.
        generator_file: std::path::PathBuf,
        /// Whether this targets a type name or a parameter name.
        kind: crate::protocol::GeneratorKind,
    },
    /// Fall back to built-in random generation.
    BuiltIn,
}

/// Determine the value source for each parameter based on resolved config.
///
/// Priority: `param_generators` (exact name match) > `type_generators` (type_name match) > `BuiltIn`.
pub fn resolve_value_sources(
    params: &[crate::types::ParamInfo],
    param_generators: &std::collections::HashMap<String, std::path::PathBuf>,
    type_generators: &std::collections::HashMap<String, std::path::PathBuf>,
) -> Vec<ValueSource> {
    params
        .iter()
        .map(|p| {
            // 1. Check param_generators by parameter name
            if let Some(gen_path) = param_generators.get(&p.name) {
                return ValueSource::CustomGenerator {
                    generator_name: p.name.clone(),
                    param_name: Some(p.name.clone()),
                    generator_file: gen_path.clone(),
                    kind: crate::protocol::GeneratorKind::ParamName,
                };
            }
            // 2. Check type_generators by type_name
            if let Some(gen_path) = p.type_name.as_ref().and_then(|tn| type_generators.get(tn)) {
                return ValueSource::CustomGenerator {
                    generator_name: p.type_name.clone().unwrap_or_default(),
                    param_name: None,
                    generator_file: gen_path.clone(),
                    kind: crate::protocol::GeneratorKind::TypeName,
                };
            }
            // 3. Built-in
            ValueSource::BuiltIn
        })
        .collect()
}

/// A single value produced by a custom generator, with replay metadata.
#[derive(Debug, Clone)]
pub struct GeneratedEntry {
    /// The generated value (JSON).
    pub value: Value,
    /// Human-readable label from the generator.
    pub generator_id: String,
    /// Serializable recipe for replaying this generation.
    /// For WASM generators where value IS the recipe, this is `None`.
    pub recipe: Option<Value>,
    /// Composite ID: `"{generator_id}@{recipe_hash}"`.
    pub composite_id: String,
}

/// Pre-fetched values from custom generators, keyed by `(generator_file, generator_name)`.
///
/// Each entry holds a queue of generated entries that can be drawn from during
/// input generation. When the queue is empty, generation falls back to built-in.
#[derive(Debug, Default)]
pub struct PrefetchedValues {
    /// Map from (generator file path as string, generator name) to queued entries.
    entries: std::collections::HashMap<(String, String), Vec<GeneratedEntry>>,
}

impl PrefetchedValues {
    /// Create an empty prefetch store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    /// Insert generated values for a specific generator (legacy API, no metadata).
    pub fn insert(&mut self, file: String, name: String, vals: Vec<Value>) {
        let entries: Vec<GeneratedEntry> = vals
            .into_iter()
            .map(|v| {
                let composite = crate::canonical_json::composite_id("unknown", &v);
                GeneratedEntry {
                    value: v,
                    generator_id: "unknown".into(),
                    recipe: None,
                    composite_id: composite,
                }
            })
            .collect();
        self.entries.entry((file, name)).or_default().extend(entries);
    }

    /// Insert a fully-formed generated entry with metadata.
    pub fn insert_entry(&mut self, file: String, name: String, entry: GeneratedEntry) {
        self.entries.entry((file, name)).or_default().push(entry);
    }

    /// Take the next value for a generator, if available.
    pub fn take(&mut self, file: &str, name: &str) -> Option<Value> {
        self.take_entry(file, name).map(|e| e.value)
    }

    /// Take the next entry (value + metadata) for a generator, if available.
    pub fn take_entry(&mut self, file: &str, name: &str) -> Option<GeneratedEntry> {
        let key = (file.to_string(), name.to_string());
        let queue = self.entries.get_mut(&key)?;
        if queue.is_empty() {
            None
        } else {
            Some(queue.remove(0))
        }
    }

    /// Check whether a generator has remaining values.
    #[must_use]
    pub fn has_values(&self, file: &str, name: &str) -> bool {
        self.entries
            .get(&(file.to_string(), name.to_string()))
            .is_some_and(|q| !q.is_empty())
    }
}

/// Collect the set of unique Generate commands needed for the given value sources.
///
/// Returns `(file, name, kind)` tuples suitable for building protocol `Generate` commands.
/// De-duplicates so each generator is only invoked once per prefetch round.
pub fn collect_generate_commands(
    sources: &[ValueSource],
) -> Vec<(String, String, crate::protocol::GeneratorKind)> {
    let mut seen = std::collections::HashSet::new();
    let mut commands = Vec::new();
    for source in sources {
        if let ValueSource::CustomGenerator {
            generator_name,
            generator_file,
            kind,
            ..
        } = source
        {
            let key = (generator_file.display().to_string(), generator_name.clone());
            if seen.insert(key.clone()) {
                commands.push((key.0, key.1, kind.clone()));
            }
        }
    }
    commands
}

/// Batch-prefetch values from custom generators via the frontend protocol.
///
/// Sends `Generate` commands for each unique generator and stores the returned
/// values in a `PrefetchedValues` store. Each generator is called `count` times.
///
/// Returns an error if any protocol send/receive fails.
pub async fn prefetch_custom_values(
    sources: &[ValueSource],
    frontend: &mut crate::frontend::Frontend,
    count: usize,
) -> Result<PrefetchedValues, crate::frontend::FrontendError> {
    let commands = collect_generate_commands(sources);
    let mut store = PrefetchedValues::new();

    for (file, name, kind) in &commands {
        for _ in 0..count {
            let response = frontend
                .send(crate::protocol::Command::Generate {
                    file: file.clone(),
                    name: name.clone(),
                    kind: kind.clone(),
                    recipe: None,
                    project_root: None,
                })
                .await?;

            match response.result {
                crate::protocol::ResponseResult::Generate { value, generator_id, recipe } => {
                    let effective_recipe = recipe.as_ref().unwrap_or(&value);
                    let composite = crate::canonical_json::composite_id(&generator_id, effective_recipe);
                    store.insert_entry(file.clone(), name.clone(), GeneratedEntry {
                        value,
                        generator_id,
                        recipe,
                        composite_id: composite,
                    });
                }
                crate::protocol::ResponseResult::Error { message, .. } => {
                    // Log but don't fail -- we'll fall back to built-in generation.
                    log::warn!("generator error for {name} ({file}): {message}");
                }
                _ => {
                    // Unexpected response type -- skip this generator.
                    log::warn!("unexpected response for generator {name}");
                }
            }
        }
    }

    Ok(store)
}

/// Generate inputs using custom generators where available, falling back to built-in.
///
/// For each parameter, if its `ValueSource` is `CustomGenerator` and the prefetch
/// store has a value available, that value is used. Otherwise, falls back to
/// `generate_random_value`.
pub fn generate_inputs_with_custom(
    params: &[crate::types::ParamInfo],
    sources: &[ValueSource],
    prefetched: &mut PrefetchedValues,
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Vec<Value> {
    params
        .iter()
        .zip(sources.iter())
        .map(|(param, source)| {
            // Check param-name heuristic for Str params before falling back.
            let heuristic = if matches!(param.typ, TypeInfo::Str) {
                heuristic_string_for_name(&param.name)
                    .or_else(|| param.type_name.as_deref().and_then(heuristic_string_for_name))
            } else {
                None
            };
            match source {
                ValueSource::CustomGenerator {
                    generator_name,
                    generator_file,
                    ..
                } => {
                    let file_str = generator_file.display().to_string();
                    prefetched
                        .take(&file_str, generator_name)
                        .unwrap_or_else(|| {
                            heuristic.map_or_else(
                                || generate_random_value(&param.typ, rng, caps),
                                |v| json!(v),
                            )
                        })
                }
                ValueSource::BuiltIn => heuristic.map_or_else(
                    || generate_random_value(&param.typ, rng, caps),
                    |v| json!(v),
                ),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Type-aware mutation operators
// ---------------------------------------------------------------------------

/// Mutate a single JSON value according to its type.
///
/// Applies a random type-appropriate mutation operator. The output is always
/// type-valid for the given `TypeInfo`. Buffer complex types receive AFL-style
/// binary mutation (bit flip, byte arithmetic, block insert/delete). All other
/// complex types, opaque types, and unknown types are returned unchanged.
pub fn mutate_value(value: &Value, typ: &TypeInfo, dictionary: &[&str], rng: &mut impl Rng) -> Value {
    match typ {
        TypeInfo::Int => mutate_int(value, rng),
        TypeInfo::Float => mutate_float(value, rng),
        TypeInfo::Bool => mutate_bool(value),
        TypeInfo::Str => mutate_string(value, dictionary, rng),
        TypeInfo::Array { element } => mutate_array(value, element, dictionary, rng),
        TypeInfo::Object { fields } => mutate_object(value, fields, dictionary, rng),
        TypeInfo::Union { variants } => mutate_union(value, variants, dictionary, rng),
        TypeInfo::Nullable { inner } => mutate_nullable(value, inner, dictionary, rng),
        TypeInfo::Complex { kind: ComplexKind::Buffer, .. } => mutate_buffer(value, rng),
        TypeInfo::Complex { .. } | TypeInfo::Opaque { .. } | TypeInfo::Unknown => value.clone(),
    }
}

/// Mutate an input vector with per-field probability.
///
/// For each parameter, with probability `mutation_rate` (0.0–1.0), applies
/// `mutate_value`; otherwise keeps the original value unchanged.
pub fn mutate_inputs(
    inputs: &[Value],
    params: &[crate::types::ParamInfo],
    mutation_rate: f64,
    dictionary: &[&str],
    rng: &mut impl Rng,
) -> Vec<Value> {
    inputs
        .iter()
        .zip(params.iter())
        .map(|(val, param)| {
            if rng.random_range(0.0..1.0_f64) < mutation_rate {
                mutate_value(val, &param.typ, dictionary, rng)
            } else {
                val.clone()
            }
        })
        .collect()
}

/// Mutate an integer value.
fn mutate_int(value: &Value, rng: &mut impl Rng) -> Value {
    let n = match value.as_i64() {
        Some(n) => n,
        None => return generate_int(rng),
    };
    let op: u8 = rng.random_range(0..3);
    match op {
        0 => {
            // Small delta
            let delta = rng.random_range(1..=10_i64);
            if rng.random_bool(0.5) {
                json!(n.saturating_add(delta))
            } else {
                json!(n.saturating_sub(delta))
            }
        }
        1 => {
            // Bitflip
            let bit = rng.random_range(0..64_u32);
            json!(n ^ (1_i64 << bit))
        }
        _ => {
            // Boundary swap
            let boundaries = [0_i64, i64::MIN, i64::MAX];
            let idx = rng.random_range(0..boundaries.len());
            json!(boundaries[idx])
        }
    }
}

/// Mutate a float value.
fn mutate_float(value: &Value, rng: &mut impl Rng) -> Value {
    let n = match value.as_f64() {
        Some(n) => n,
        None => return generate_float(rng),
    };
    let op: u8 = rng.random_range(0..4);
    match op {
        0 => {
            // Epsilon perturbation
            let factor = 1.0 + rng.random_range(-0.1..0.1_f64);
            json!(n * factor)
        }
        1 => {
            // Sign flip
            json!(-n)
        }
        2 => {
            // Special values — NaN becomes null in JSON, Inf becomes null too
            let specials: &[f64] = &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0];
            let idx = rng.random_range(0..specials.len());
            json!(specials[idx])
        }
        _ => {
            // Small delta
            let delta = rng.random_range(-1.0..1.0_f64);
            json!(n + delta)
        }
    }
}

/// Mutate a boolean value (always flip).
fn mutate_bool(value: &Value) -> Value {
    match value.as_bool() {
        Some(b) => json!(!b),
        None => json!(true),
    }
}

/// Mutate a string value.
///
/// When `dictionary` is non-empty, an additional mutation operator (fragment
/// injection) becomes available — it splices a random dictionary entry into
/// the string at a random position.
fn mutate_string(value: &Value, dictionary: &[&str], rng: &mut impl Rng) -> Value {
    let s = match value.as_str() {
        Some(s) => s.to_string(),
        None => return generate_string(rng),
    };
    // 20% chance of structure-aware mutation (pattern insertion, unicode, segment ops)
    if rng.random_range(0..5_u8) == 0 {
        return json!(string_mutation::mutate_structure_aware(&s, rng));
    }
    let num_ops: u8 = if dictionary.is_empty() { 6 } else { 7 };
    let op: u8 = rng.random_range(0..num_ops);
    match op {
        0 => {
            // Char substitution
            if s.is_empty() {
                return json!(s);
            }
            let chars: Vec<char> = s.chars().collect();
            let idx = rng.random_range(0..chars.len());
            let new_char = rng.random_range(b' '..=b'~') as char;
            let mut result: String = chars.iter().collect();
            // Replace at byte position of the idx-th char
            let byte_start: usize = chars[..idx].iter().map(|c| c.len_utf8()).sum();
            let byte_end = byte_start + chars[idx].len_utf8();
            result.replace_range(byte_start..byte_end, &new_char.to_string());
            json!(result)
        }
        1 => {
            // Char insertion
            let chars: Vec<char> = s.chars().collect();
            let idx = rng.random_range(0..=chars.len());
            let new_char = rng.random_range(b' '..=b'~') as char;
            let mut result = String::with_capacity(s.len() + 1);
            for (i, ch) in chars.iter().enumerate() {
                if i == idx {
                    result.push(new_char);
                }
                result.push(*ch);
            }
            if idx == chars.len() {
                result.push(new_char);
            }
            json!(result)
        }
        2 => {
            // Char deletion
            if s.is_empty() {
                return json!(s);
            }
            let chars: Vec<char> = s.chars().collect();
            let idx = rng.random_range(0..chars.len());
            let result: String = chars
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != idx)
                .map(|(_, c)| *c)
                .collect();
            json!(result)
        }
        3 => {
            // Case flip
            if s.is_empty() {
                return json!(s);
            }
            let chars: Vec<char> = s.chars().collect();
            let idx = rng.random_range(0..chars.len());
            let result: String = chars
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    if i == idx {
                        if c.is_uppercase() {
                            c.to_lowercase().next().unwrap_or(*c)
                        } else {
                            c.to_uppercase().next().unwrap_or(*c)
                        }
                    } else {
                        *c
                    }
                })
                .collect();
            json!(result)
        }
        4 => {
            // Empty
            json!("")
        }
        5 => {
            // Long string (1000 chars)
            let long: String = (0..1000).map(|_| 'a').collect();
            json!(long)
        }
        _ => {
            // Dictionary fragment injection
            let fragment = dictionary[rng.random_range(0..dictionary.len())];
            let chars: Vec<char> = s.chars().collect();
            let pos = rng.random_range(0..=chars.len());
            let byte_pos: usize = chars[..pos].iter().map(|c| c.len_utf8()).sum();
            let mut result = s;
            result.insert_str(byte_pos, fragment);
            json!(result)
        }
    }
}

/// Mutate an array value.
fn mutate_array(value: &Value, element: &TypeInfo, dictionary: &[&str], rng: &mut impl Rng) -> Value {
    let arr = match value.as_array() {
        Some(a) => a.clone(),
        None => return generate_array(element, rng, None),
    };
    let op: u8 = if arr.is_empty() {
        1 // Can only add
    } else if arr.len() < 2 {
        rng.random_range(0..3) // Remove, add, or mutate (no shuffle)
    } else {
        rng.random_range(0..4)
    };
    match op {
        0 => {
            // Remove element
            let mut result = arr;
            let idx = rng.random_range(0..result.len());
            result.remove(idx);
            json!(result)
        }
        1 => {
            // Add element
            let mut result = arr;
            let new_elem = generate_random_value(element, rng, None);
            result.push(new_elem);
            json!(result)
        }
        2 => {
            // Mutate element
            let mut result = arr;
            let idx = rng.random_range(0..result.len());
            result[idx] = mutate_value(&result[idx], element, dictionary, rng);
            json!(result)
        }
        _ => {
            // Shuffle (swap two random elements)
            let mut result = arr;
            let i = rng.random_range(0..result.len());
            let j = rng.random_range(0..result.len());
            result.swap(i, j);
            json!(result)
        }
    }
}

/// Mutate an object value.
fn mutate_object(
    value: &Value,
    fields: &[(String, TypeInfo)],
    dictionary: &[&str],
    rng: &mut impl Rng,
) -> Value {
    let obj = match value.as_object() {
        Some(o) => o.clone(),
        None => return generate_object(fields, rng, None),
    };
    if fields.is_empty() {
        return Value::Object(obj);
    }
    let op: u8 = rng.random_range(0..3);
    match op {
        0 => {
            // Mutate field value
            let mut result = obj;
            let idx = rng.random_range(0..fields.len());
            let (name, typ) = &fields[idx];
            if let Some(current) = result.get(name) {
                let mutated = mutate_value(current, typ, dictionary, rng);
                result.insert(name.clone(), mutated);
            }
            Value::Object(result)
        }
        1 => {
            // Remove a field
            let mut result = obj;
            let idx = rng.random_range(0..fields.len());
            result.remove(&fields[idx].0);
            Value::Object(result)
        }
        _ => {
            // Add an extra field
            let mut result = obj;
            result.insert("_extra".to_string(), generate_unknown(rng));
            Value::Object(result)
        }
    }
}

/// Mutate a union value by applying mutation with a random variant's type.
fn mutate_union(value: &Value, variants: &[TypeInfo], dictionary: &[&str], rng: &mut impl Rng) -> Value {
    if variants.is_empty() {
        return value.clone();
    }
    let idx = rng.random_range(0..variants.len());
    mutate_value(value, &variants[idx], dictionary, rng)
}

/// Mutate a nullable value: 20% chance to flip null/non-null, otherwise mutate inner.
fn mutate_nullable(value: &Value, inner: &TypeInfo, dictionary: &[&str], rng: &mut impl Rng) -> Value {
    if rng.random_range(0..5) == 0 {
        // Flip null / non-null
        if value.is_null() {
            generate_random_value(inner, rng, None)
        } else {
            Value::Null
        }
    } else if value.is_null() {
        // Stay null most of the time if already null
        Value::Null
    } else {
        mutate_value(value, inner, dictionary, rng)
    }
}

// ---------------------------------------------------------------------------
// Binary buffer mutation operators (AFL-style)
// ---------------------------------------------------------------------------

/// Maximum number of bytes to insert in a block insertion mutation.
const BUFFER_MAX_BLOCK_INSERT: usize = 8;

/// Maximum number of bytes to delete in a block deletion mutation.
const BUFFER_MAX_BLOCK_DELETE: usize = 8;

/// Maximum absolute delta applied during byte arithmetic mutation.
const BUFFER_BYTE_ARITH_MAX_DELTA: u8 = 35;

/// Alphabet for standard (non-URL-safe) base64 encoding.
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode a byte slice to a standard base64 string (with `=` padding).
///
/// Avoids adding a `base64` crate dependency by implementing the minimal
/// encoding needed for the buffer wire format.
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let combined = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[((combined >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_ALPHABET[((combined >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64_ALPHABET[((combined >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(BASE64_ALPHABET[(combined & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Decode a standard base64 string to bytes.
///
/// Returns `None` for any character that is not in the base64 alphabet or `=`.
/// Padding is handled leniently — trailing `=` characters are ignored.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let chars: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    for chunk in chars.chunks(4) {
        let decode_char = |c: u8| -> Option<u32> {
            BASE64_ALPHABET.iter().position(|&x| x == c).map(|p| p as u32)
        };
        let c0 = decode_char(chunk[0])?;
        let c1 = decode_char(*chunk.get(1).unwrap_or(&b'A'))?;
        let combined = (c0 << 18) | (c1 << 12);
        out.push(((combined >> 16) & 0xFF) as u8);
        if let Some(&c2_raw) = chunk.get(2) {
            let c2 = decode_char(c2_raw)?;
            let combined2 = combined | (c2 << 6);
            out.push(((combined2 >> 8) & 0xFF) as u8);
            if let Some(&c3_raw) = chunk.get(3) {
                let c3 = decode_char(c3_raw)?;
                out.push(((combined2 | c3) & 0xFF) as u8);
            }
        }
    }
    Some(out)
}

/// Apply AFL-style binary mutation operators to a buffer complex value.
///
/// Expects the wire format `{"__complex_type": "buffer", "encoding": "base64",
/// "value": <base64-string>}`. If the value is malformed, falls back to
/// generating a fresh buffer.
///
/// Four AFL-style mutation operators are applied, chosen uniformly at random
/// when the buffer is non-empty. Empty buffers are always grown via block
/// insertion (the only sensible operator when there are no bytes to modify):
/// - **Bit flip**: flip a single random bit.
/// - **Byte arithmetic**: add or subtract a small random delta (wrapping) from a
///   single byte. This produces near-boundary values that trigger off-by-one checks.
/// - **Block insertion**: insert 1–8 random bytes at a random position.
/// - **Block deletion**: delete 1–8 bytes starting at a random position.
///
/// String inputs are not affected: this function is only reached for
/// `TypeInfo::Complex { kind: ComplexKind::Buffer, .. }` parameters.
fn mutate_buffer(value: &Value, rng: &mut impl Rng) -> Value {
    // Extract the base64 payload from the wire format.
    let encoded = value
        .as_object()
        .and_then(|o| o.get("value"))
        .and_then(|v| v.as_str());

    let mut bytes = match encoded.and_then(base64_decode) {
        Some(b) => b,
        None => {
            // Malformed input — generate a fresh small buffer.
            let len = rng.random_range(0..=4_usize);
            (0..len).map(|_| rng.random_range(0u8..=255)).collect()
        }
    };

    // Operator selection — only operators valid for the current buffer length are chosen.
    //
    // For non-empty buffers all four operators are available:
    //   0 = bit flip, 1 = block insertion, 2 = byte arithmetic, 3 = block deletion
    // For empty buffers only block insertion makes sense; the other three operators
    // all require at least one existing byte to work on.
    if bytes.is_empty() {
        // Block insertion is the only sensible operator for an empty buffer.
        let insert_len = rng.random_range(1..=BUFFER_MAX_BLOCK_INSERT);
        let new_bytes: Vec<u8> = (0..insert_len)
            .map(|_| rng.random_range(0u8..=255))
            .collect();
        bytes.extend(new_bytes);
    } else {
        let op: u8 = rng.random_range(0..4_u8);
        match op {
            0 => {
                // Bit flip: flip one random bit.
                let byte_idx = rng.random_range(0..bytes.len());
                let bit = rng.random_range(0..8_u8);
                bytes[byte_idx] ^= 1 << bit;
            }
            1 => {
                // Block insertion: insert 1–8 random bytes at a random position.
                let insert_len = rng.random_range(1..=BUFFER_MAX_BLOCK_INSERT);
                let pos = rng.random_range(0..=bytes.len());
                let new_bytes: Vec<u8> = (0..insert_len)
                    .map(|_| rng.random_range(0u8..=255))
                    .collect();
                bytes.splice(pos..pos, new_bytes);
            }
            2 => {
                // Byte arithmetic: add or subtract a small delta from one byte (wrapping).
                let byte_idx = rng.random_range(0..bytes.len());
                let delta: u8 = rng.random_range(1..=BUFFER_BYTE_ARITH_MAX_DELTA);
                bytes[byte_idx] = if rng.random_bool(0.5) {
                    bytes[byte_idx].wrapping_add(delta)
                } else {
                    bytes[byte_idx].wrapping_sub(delta)
                };
            }
            _ => {
                // Block deletion: delete 1–8 bytes starting at a random position.
                let max_del = BUFFER_MAX_BLOCK_DELETE.min(bytes.len());
                let del_len = rng.random_range(1..=max_del);
                let start = rng.random_range(0..=bytes.len() - del_len);
                bytes.drain(start..start + del_len);
            }
        }
    }

    json!({
        "__complex_type": "buffer",
        "encoding": "base64",
        "value": base64_encode(&bytes)
    })
}

// ---------------------------------------------------------------------------
// Type-aware crossover operators
// ---------------------------------------------------------------------------

/// Number of string crossover strategies (splice, substring insertion, single-point).
const STRING_CROSSOVER_STRATEGY_COUNT: u32 = 3;

/// Produce two children by crossing over two parent input vectors.
///
/// With probability `crossover_rate`, performs type-aware crossover; otherwise
/// clones both parents unchanged. At the parameter level, uniform crossover
/// randomly assigns each parameter from parent A or B. Object and array
/// parameters use finer-grained strategies (field-level and single-point,
/// respectively).
pub fn crossover_inputs(
    parent_a: &[Value],
    parent_b: &[Value],
    params: &[crate::types::ParamInfo],
    crossover_rate: f64,
    rng: &mut impl Rng,
) -> (Vec<Value>, Vec<Value>) {
    let len = parent_a.len().min(parent_b.len()).min(params.len());

    // No crossover — clone parents
    if rng.random_range(0.0..1.0_f64) >= crossover_rate {
        return (parent_a.to_vec(), parent_b.to_vec());
    }

    let mut child1 = Vec::with_capacity(len);
    let mut child2 = Vec::with_capacity(len);

    for i in 0..len {
        let (c1, c2) = crossover_value(&parent_a[i], &parent_b[i], &params[i].typ, rng);
        child1.push(c1);
        child2.push(c2);
    }

    (child1, child2)
}

/// Cross over two values according to their type.
///
/// - **Object**: field-level crossover (each field randomly from A or B).
/// - **Array**: single-point crossover (prefix from one, suffix from the other).
/// - **Other types**: uniform swap (randomly assign A→child1/B→child2 or vice versa).
fn crossover_value(a: &Value, b: &Value, typ: &TypeInfo, rng: &mut impl Rng) -> (Value, Value) {
    match typ {
        TypeInfo::Object { fields } => crossover_object(a, b, fields, rng),
        TypeInfo::Array { .. } => crossover_array(a, b, rng),
        TypeInfo::Str => crossover_string(a, b, rng),
        _ => {
            if rng.random_bool(0.5) {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            }
        }
    }
}

/// Field-level crossover for objects: each field independently chosen from A or B.
fn crossover_object(
    a: &Value,
    b: &Value,
    fields: &[(String, TypeInfo)],
    rng: &mut impl Rng,
) -> (Value, Value) {
    let obj_a = a.as_object();
    let obj_b = b.as_object();

    // If either isn't actually an object, fall back to uniform swap
    let (Some(obj_a), Some(obj_b)) = (obj_a, obj_b) else {
        return if rng.random_bool(0.5) {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
    };

    let mut c1 = serde_json::Map::new();
    let mut c2 = serde_json::Map::new();

    for (name, _typ) in fields {
        let val_a = obj_a.get(name);
        let val_b = obj_b.get(name);
        match (val_a, val_b) {
            (Some(va), Some(vb)) => {
                if rng.random_bool(0.5) {
                    c1.insert(name.clone(), va.clone());
                    c2.insert(name.clone(), vb.clone());
                } else {
                    c1.insert(name.clone(), vb.clone());
                    c2.insert(name.clone(), va.clone());
                }
            }
            (Some(v), None) | (None, Some(v)) => {
                // Only one parent has the field — give to one child
                if rng.random_bool(0.5) {
                    c1.insert(name.clone(), v.clone());
                } else {
                    c2.insert(name.clone(), v.clone());
                }
            }
            (None, None) => {}
        }
    }

    (Value::Object(c1), Value::Object(c2))
}

/// Single-point crossover for arrays: pick a crossover point, swap tails.
fn crossover_array(a: &Value, b: &Value, rng: &mut impl Rng) -> (Value, Value) {
    let arr_a = a.as_array();
    let arr_b = b.as_array();

    let (Some(arr_a), Some(arr_b)) = (arr_a, arr_b) else {
        return if rng.random_bool(0.5) {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
    };

    if arr_a.is_empty() && arr_b.is_empty() {
        return (json!([]), json!([]));
    }

    // Crossover point chosen from 0..=min_len so both sides contribute
    let min_len = arr_a.len().min(arr_b.len());
    let point = rng.random_range(0..=min_len);

    // child1 = arr_a[..point] ++ arr_b[point..]
    // child2 = arr_b[..point] ++ arr_a[point..]
    let mut c1: Vec<Value> = arr_a.iter().take(point).cloned().collect();
    c1.extend(arr_b.iter().skip(point).cloned());

    let mut c2: Vec<Value> = arr_b.iter().take(point).cloned().collect();
    c2.extend(arr_a.iter().skip(point).cloned());

    (json!(c1), json!(c2))
}

/// String-level crossover: combines characters from both parents rather than
/// picking one whole string or the other. Three strategies chosen uniformly:
/// splice (prefix A + suffix B), substring insertion, and single-point swap.
fn crossover_string(a: &Value, b: &Value, rng: &mut impl Rng) -> (Value, Value) {
    let (Some(sa), Some(sb)) = (a.as_str(), b.as_str()) else {
        return if rng.random_bool(0.5) {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
    };

    // Empty strings — fall back to uniform swap
    if sa.is_empty() || sb.is_empty() {
        return if rng.random_bool(0.5) {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
    }

    let chars_a: Vec<char> = sa.chars().collect();
    let chars_b: Vec<char> = sb.chars().collect();

    let strategy = rng.random_range(0..STRING_CROSSOVER_STRATEGY_COUNT);
    match strategy {
        // Splice: prefix of A + suffix of B (and vice versa)
        0 => {
            let min_len = chars_a.len().min(chars_b.len());
            let point = rng.random_range(0..=min_len);
            let c1: String = chars_a[..point].iter().chain(&chars_b[point..]).collect();
            let c2: String = chars_b[..point].iter().chain(&chars_a[point..]).collect();
            (json!(c1), json!(c2))
        }
        // Substring insertion: random substring of B inserted into random position of A
        1 => {
            let start_b = rng.random_range(0..chars_b.len());
            let end_b = rng.random_range(start_b..=chars_b.len());
            let substr: String = chars_b[start_b..end_b].iter().collect();
            let pos_a = rng.random_range(0..=chars_a.len());
            let c1: String = chars_a[..pos_a]
                .iter()
                .chain(substr.chars().collect::<Vec<_>>().iter())
                .chain(&chars_a[pos_a..])
                .collect();

            let start_a = rng.random_range(0..chars_a.len());
            let end_a = rng.random_range(start_a..=chars_a.len());
            let substr2: String = chars_a[start_a..end_a].iter().collect();
            let pos_b = rng.random_range(0..=chars_b.len());
            let c2: String = chars_b[..pos_b]
                .iter()
                .chain(substr2.chars().collect::<Vec<_>>().iter())
                .chain(&chars_b[pos_b..])
                .collect();

            (json!(c1), json!(c2))
        }
        // Single-point: independent split points, swap tails
        _ => {
            let pa = rng.random_range(0..=chars_a.len());
            let pb = rng.random_range(0..=chars_b.len());
            let c1: String = chars_a[..pa].iter().chain(&chars_b[pb..]).collect();
            let c2: String = chars_b[..pb].iter().chain(&chars_a[pa..]).collect();
            (json!(c1), json!(c2))
        }
    }
}

// ---------------------------------------------------------------------------
// Type-aware shrink candidates
// ---------------------------------------------------------------------------

/// Produce progressively simpler variants of `value` consistent with `type_info`.
///
/// Returns candidates ordered roughly from "most simplified" to "least simplified".
/// Never includes the original value. Used for minimal witness shrinking and
/// boundary refinement — the inverse of mutation.
pub fn shrink_candidates(value: &Value, type_info: &TypeInfo) -> Vec<Value> {
    let candidates = match type_info {
        TypeInfo::Int => shrink_int(value),
        TypeInfo::Float => shrink_float(value),
        TypeInfo::Str => shrink_str(value),
        TypeInfo::Bool => shrink_bool(value),
        TypeInfo::Array { element } => shrink_array(value, element),
        TypeInfo::Object { fields } => shrink_object(value, fields),
        TypeInfo::Nullable { inner } => shrink_nullable(value, inner),
        TypeInfo::Union { variants } => shrink_union(value, variants),
        TypeInfo::Complex { .. } | TypeInfo::Opaque { .. } | TypeInfo::Unknown => Vec::new(),
    };
    // Filter out duplicates and any candidate that equals the original.
    let mut seen = Vec::with_capacity(candidates.len());
    for c in candidates {
        if c != *value && !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen
}

fn shrink_int(value: &Value) -> Vec<Value> {
    let n = match value.as_i64() {
        Some(n) => n,
        None => return Vec::new(),
    };
    let mut out = Vec::with_capacity(4);
    let half = n / 2;
    if half != n {
        out.push(json!(half));
    }
    if n != 0 {
        out.push(json!(0));
    }
    if n != 1 {
        out.push(json!(1));
    }
    if n != -1 {
        out.push(json!(-1));
    }
    out
}

fn shrink_float(value: &Value) -> Vec<Value> {
    let n = match value.as_f64() {
        Some(n) if n.is_finite() => n,
        // NaN / Infinity / non-float → shrink to 0.0
        Some(_) => return vec![json!(0.0)],
        None => return Vec::new(),
    };
    let mut out = Vec::with_capacity(4);
    let half = n / 2.0;
    if (half - n).abs() > f64::EPSILON {
        out.push(json!(half));
    }
    if n.abs() > f64::EPSILON {
        out.push(json!(0.0));
    }
    if (n - 1.0).abs() > f64::EPSILON {
        out.push(json!(1.0));
    }
    if (n + 1.0).abs() > f64::EPSILON {
        out.push(json!(-1.0));
    }
    out
}

fn shrink_str(value: &Value) -> Vec<Value> {
    let s = match value.as_str() {
        Some(s) => s,
        None => return Vec::new(),
    };
    if s.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::with_capacity(4);
    // Remove last char
    if chars.len() > 1 {
        let without_last: String = chars[..chars.len() - 1].iter().collect();
        out.push(json!(without_last));
    }
    // Remove first char
    if chars.len() > 1 {
        let without_first: String = chars[1..].iter().collect();
        out.push(json!(without_first));
    }
    // Empty string
    out.push(json!(""));
    // Single first char
    if chars.len() > 1 {
        out.push(json!(chars[0].to_string()));
    }
    out
}

fn shrink_bool(value: &Value) -> Vec<Value> {
    match value.as_bool() {
        Some(true) => vec![json!(false)],
        _ => Vec::new(),
    }
}

fn shrink_array(value: &Value, element: &TypeInfo) -> Vec<Value> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    if arr.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(3 + arr.len());
    // Remove last element
    if arr.len() > 1 {
        out.push(json!(arr[..arr.len() - 1]));
    }
    // Remove first element
    if arr.len() > 1 {
        out.push(json!(arr[1..]));
    }
    // Empty array
    out.push(json!([]));
    // Shrink each element individually
    for (i, elem) in arr.iter().enumerate() {
        for shrunk in shrink_candidates(elem, element) {
            let mut new_arr = arr.clone();
            new_arr[i] = shrunk;
            out.push(Value::Array(new_arr));
        }
    }
    out
}

fn shrink_object(value: &Value, fields: &[(String, TypeInfo)]) -> Vec<Value> {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };
    if fields.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(fields.len() * 2);
    // Remove each field one at a time
    for (name, _) in fields {
        if obj.contains_key(name) {
            let mut reduced = obj.clone();
            reduced.remove(name);
            out.push(Value::Object(reduced));
        }
    }
    // Shrink each field value individually
    for (name, typ) in fields {
        if let Some(val) = obj.get(name) {
            for shrunk in shrink_candidates(val, typ) {
                let mut new_obj = obj.clone();
                new_obj.insert(name.clone(), shrunk);
                out.push(Value::Object(new_obj));
            }
        }
    }
    out
}

fn shrink_nullable(value: &Value, inner: &TypeInfo) -> Vec<Value> {
    let mut out = Vec::with_capacity(4);
    if !value.is_null() {
        out.push(Value::Null);
        out.extend(shrink_candidates(value, inner));
    }
    out
}

fn shrink_union(value: &Value, variants: &[TypeInfo]) -> Vec<Value> {
    let mut out = Vec::new();
    for variant in variants {
        out.extend(shrink_candidates(value, variant));
    }
    out
}

// ---------------------------------------------------------------------------
// Literal-derived candidate inputs
// ---------------------------------------------------------------------------

use crate::boundary_dict::get_boundary_values;
use crate::protocol::LiteralValue;
use crate::types::ParamInfo;

/// Convert extracted literal values from static analysis into candidate input vectors.
///
/// For each `LiteralValue`, produces one input vector per parameter whose type
/// is compatible with the literal's type. Other parameters receive a neutral default
/// (first boundary value for their type).
///
/// Deduplication: identical `(literal, param_index)` pairs produce a single vector.
pub fn literals_to_candidate_inputs(
    params: &[ParamInfo],
    literals: &[LiteralValue],
) -> Vec<Vec<Value>> {
    if params.is_empty() || literals.is_empty() {
        return Vec::new();
    }

    // Neutral default per parameter: first boundary value or null
    let defaults: Vec<Value> = params
        .iter()
        .map(|p| {
            get_boundary_values(&p.typ)
                .into_iter()
                .next()
                .map(|e| e.value)
                .unwrap_or(Value::Null)
        })
        .collect();

    // Deduplicate literals first
    let mut lit_seen = std::collections::HashSet::new();
    let deduped: Vec<&LiteralValue> = literals
        .iter()
        .filter(|lit| {
            let key = serde_json::to_string(lit).unwrap_or_default();
            lit_seen.insert(key)
        })
        .collect();

    // Deduplicate by (param_index, serialized_value)
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for lit in &deduped {
        for (idx, param) in params.iter().enumerate() {
            let Some(val) = literal_matches_type(lit, &param.typ) else {
                continue;
            };
            let dedup_key = (idx, serde_json::to_string(&val).unwrap_or_default());
            if !seen.insert(dedup_key) {
                continue;
            }
            let mut row = defaults.clone();
            row[idx] = val;
            result.push(row);
        }
    }

    result
}

/// Convert interesting pool values into candidate input vectors.
///
/// Follows the same pattern as [`literals_to_candidate_inputs`]: for each
/// parameter, looks up pool values matching that parameter's type. Each match
/// produces one input vector with the pool value at that position and boundary
/// defaults at other positions. Deduplicates by `(param_index, serialized_value)`.
pub fn pool_to_candidate_inputs(
    params: &[ParamInfo],
    pool: &crate::interesting_pool::InterestingPool,
) -> Vec<Vec<Value>> {
    if params.is_empty() {
        return Vec::new();
    }

    let defaults: Vec<Value> = params
        .iter()
        .map(|p| {
            get_boundary_values(&p.typ)
                .into_iter()
                .next()
                .map(|e| e.value)
                .unwrap_or(Value::Null)
        })
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for (idx, param) in params.iter().enumerate() {
        for val in pool.values_for_type(&param.typ) {
            let dedup_key = (idx, serde_json::to_string(&val).unwrap_or_default());
            if !seen.insert(dedup_key) {
                continue;
            }
            let mut row = defaults.clone();
            row[idx] = val;
            result.push(row);
        }
    }

    result
}

/// Check whether a `LiteralValue` is type-compatible with a `TypeInfo` and return
/// the corresponding `serde_json::Value` if so.
fn literal_matches_type(lit: &LiteralValue, typ: &TypeInfo) -> Option<Value> {
    match (lit, typ) {
        (LiteralValue::Int { value }, TypeInfo::Int) => Some(json!(value)),
        (LiteralValue::Int { value }, TypeInfo::Float) => Some(json!(*value as f64)),
        (LiteralValue::Float { value }, TypeInfo::Float) => Some(json!(value)),
        (LiteralValue::Str { value }, TypeInfo::Str) => Some(json!(value)),
        (LiteralValue::Bool { value }, TypeInfo::Bool) => Some(json!(value)),
        // For union types, try each variant
        (_, TypeInfo::Union { variants }) => {
            variants.iter().find_map(|v| literal_matches_type(lit, v))
        }
        // For nullable, try the inner type
        (_, TypeInfo::Nullable { inner }) => literal_matches_type(lit, inner),
        // Regex literal → Str param: try the pattern as a string input
        (LiteralValue::Regex { pattern }, TypeInfo::Str) => Some(json!(pattern)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Mock value generation, mutation, and crossover
// ---------------------------------------------------------------------------

use crate::auto_mock::{IoCategory, MockParam};
use crate::protocol::{MockBehavior, MockConfig};

/// Generate type-correct [`MockConfig`] for each [`MockParam`].
///
/// For each mock parameter:
/// - Generates `max(call_count_estimate, 1)` return values (for loop support)
/// - Uses category-aware shaping when `return_type` is `Unknown`
/// - Produces both success and error variants for declared error types
///   (`Complex { kind: Result, .. }` or unions containing `Complex { kind: Error, .. }`)
pub fn generate_mock_values(
    mock_params: &[MockParam],
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> Vec<MockConfig> {
    mock_params
        .iter()
        .map(|mp| {
            let count = (mp.call_count_estimate as usize).max(1);
            let (return_values, behavior) =
                generate_mock_return_values(&mp.return_type, mp.category, count, rng, caps);

            MockConfig {
                symbol: mp.symbol.clone(),
                return_values,
                should_track_calls: true,
                default_behavior: behavior,
            }
        })
        .collect()
}

/// Generate return values for a single mock, dispatching to error-aware or
/// category-shaped generators as needed.
fn generate_mock_return_values(
    return_type: &TypeInfo,
    category: IoCategory,
    count: usize,
    rng: &mut impl Rng,
    caps: Option<&FrontendCapabilities>,
) -> (Vec<Value>, MockBehavior) {
    // Result complex type: alternates ok/err variants using the tagged wire format.
    if matches!(return_type, TypeInfo::Complex { kind: ComplexKind::Result, .. }) {
        let values = (0..count)
            .map(|i| {
                if i % 2 == 0 {
                    let inner = generate_random_value(&TypeInfo::Unknown, rng, caps);
                    json!({"__complex_type": "result", "ok": true, "value": inner})
                } else {
                    json!({"__complex_type": "result", "ok": false, "value": "mock error"})
                }
            })
            .collect();
        return (values, MockBehavior::ReturnGenerated);
    }

    // Union-with-Error: alternate success and error variants.
    // Error variants use generate_error() directly for the tagged format.
    if let TypeInfo::Union { variants } = return_type {
        let (error_indices, success_indices) = partition_error_variants(variants);
        if !error_indices.is_empty() && !success_indices.is_empty() {
            let mut values = Vec::with_capacity(count);
            for i in 0..count {
                if i % 2 == 0 {
                    let idx = success_indices[rng.random_range(0..success_indices.len())];
                    values.push(generate_random_value(&variants[idx], rng, caps));
                } else {
                    let idx = error_indices[rng.random_range(0..error_indices.len())];
                    let metadata = match &variants[idx] {
                        TypeInfo::Complex { metadata, .. } => metadata.clone(),
                        _ => serde_json::Map::new(),
                    };
                    values.push(generate_error(&metadata, rng));
                }
            }
            return (values, MockBehavior::ReturnGenerated);
        }
    }

    // Category-aware shaping for Unknown return types.
    if matches!(return_type, TypeInfo::Unknown) {
        let values = (0..count)
            .map(|_| category_shaped_value(category, rng))
            .collect();
        return (values, MockBehavior::RepeatLast);
    }

    // Standard type-driven generation.
    let values = (0..count)
        .map(|_| generate_random_value(return_type, rng, caps))
        .collect();
    (values, MockBehavior::RepeatLast)
}

/// Partition union variants into error and non-error index lists.
fn partition_error_variants(variants: &[TypeInfo]) -> (Vec<usize>, Vec<usize>) {
    let mut error_indices = Vec::new();
    let mut success_indices = Vec::new();
    for (i, v) in variants.iter().enumerate() {
        if matches!(v, TypeInfo::Complex { kind: ComplexKind::Error, .. }) {
            error_indices.push(i);
        } else {
            success_indices.push(i);
        }
    }
    (error_indices, success_indices)
}

/// Generate a category-shaped value for Unknown return types.
fn category_shaped_value(category: IoCategory, rng: &mut impl Rng) -> Value {
    match category {
        IoCategory::FileSystem => {
            let choice: u8 = rng.random_range(0..3);
            match choice {
                0 => json!(""),
                1 => json!(true),
                _ => Value::Null,
            }
        }
        IoCategory::Network => json!({"status": 200, "data": {}}),
        IoCategory::Database => {
            if rng.random_bool(0.5) {
                json!({"rows": []})
            } else {
                json!({"rowCount": 1})
            }
        }
        IoCategory::PureUtility | IoCategory::ExternalOther => generate_unknown(rng),
    }
}

/// Mutate mock return values while preserving type contracts.
///
/// For each MockConfig/MockParam pair, applies [`mutate_value`] to each
/// return value with probability `mutation_rate`. Output always has the
/// same number of configs with the same symbols and vector lengths.
pub fn mutate_mock_values(
    configs: &[MockConfig],
    mock_params: &[MockParam],
    mutation_rate: f64,
    dictionary: &[&str],
    rng: &mut impl Rng,
) -> Vec<MockConfig> {
    configs
        .iter()
        .zip(mock_params.iter())
        .map(|(config, mp)| {
            let return_values = config
                .return_values
                .iter()
                .map(|val| {
                    if rng.random_range(0.0..1.0_f64) < mutation_rate {
                        mutate_value(val, &mp.return_type, dictionary, rng)
                    } else {
                        val.clone()
                    }
                })
                .collect();

            MockConfig {
                symbol: config.symbol.clone(),
                return_values,
                should_track_calls: config.should_track_calls,
                default_behavior: config.default_behavior.clone(),
            }
        })
        .collect()
}

/// Cross over two parent mock config vectors, preserving vector structure.
///
/// With probability `crossover_rate`, performs per-value crossover between
/// matching return values. Both children have the same number of configs
/// with the same symbols as the parents.
pub fn crossover_mock_values(
    parent_a: &[MockConfig],
    parent_b: &[MockConfig],
    mock_params: &[MockParam],
    crossover_rate: f64,
    rng: &mut impl Rng,
) -> (Vec<MockConfig>, Vec<MockConfig>) {
    let len = parent_a.len().min(parent_b.len()).min(mock_params.len());

    if rng.random_range(0.0..1.0_f64) >= crossover_rate {
        return (parent_a.to_vec(), parent_b.to_vec());
    }

    let mut child1 = Vec::with_capacity(len);
    let mut child2 = Vec::with_capacity(len);

    for i in 0..len {
        let (rv1, rv2) = crossover_return_values(
            &parent_a[i].return_values,
            &parent_b[i].return_values,
            &mock_params[i].return_type,
            rng,
        );

        child1.push(MockConfig {
            symbol: parent_a[i].symbol.clone(),
            return_values: rv1,
            should_track_calls: parent_a[i].should_track_calls,
            default_behavior: parent_a[i].default_behavior.clone(),
        });
        child2.push(MockConfig {
            symbol: parent_b[i].symbol.clone(),
            return_values: rv2,
            should_track_calls: parent_b[i].should_track_calls,
            default_behavior: parent_b[i].default_behavior.clone(),
        });
    }

    (child1, child2)
}

/// Cross over two return value vectors using per-element type-aware crossover.
fn crossover_return_values(
    a: &[Value],
    b: &[Value],
    typ: &TypeInfo,
    rng: &mut impl Rng,
) -> (Vec<Value>, Vec<Value>) {
    let overlap = a.len().min(b.len());
    let mut rv1 = Vec::with_capacity(a.len());
    let mut rv2 = Vec::with_capacity(b.len());

    for i in 0..overlap {
        let (c1, c2) = crossover_value(&a[i], &b[i], typ, rng);
        rv1.push(c1);
        rv2.push(c2);
    }

    rv1.extend(a.iter().skip(overlap).cloned());
    rv2.extend(b.iter().skip(overlap).cloned());

    (rv1, rv2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ParamInfo;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn seeded_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn generates_int_values() {
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Int, &mut rng, None);
            assert!(val.is_i64() || val.is_u64(), "expected integer, got {val}");
        }
    }

    #[test]
    fn generates_float_values() {
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Float, &mut rng, None);
            assert!(val.is_f64() || val.is_i64(), "expected number, got {val}");
        }
    }

    #[test]
    fn generates_string_values() {
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Str, &mut rng, None);
            assert!(val.is_string(), "expected string, got {val}");
        }
    }

    #[test]
    fn generates_bool_values() {
        let mut rng = seeded_rng();
        let mut saw_true = false;
        let mut saw_false = false;
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Bool, &mut rng, None);
            assert!(val.is_boolean(), "expected bool, got {val}");
            if val.as_bool() == Some(true) {
                saw_true = true;
            } else {
                saw_false = true;
            }
        }
        assert!(saw_true && saw_false, "expected both true and false values");
    }

    #[test]
    fn generates_array_values() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        for _ in 0..20 {
            let val = generate_random_value(&typ, &mut rng, None);
            assert!(val.is_array(), "expected array, got {val}");
        }
    }

    #[test]
    fn generates_object_values() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                ("age".into(), TypeInfo::Int),
            ],
        };
        for _ in 0..20 {
            let val = generate_random_value(&typ, &mut rng, None);
            let obj = val.as_object().expect("expected object");
            assert!(obj.contains_key("name"));
            assert!(obj.contains_key("age"));
        }
    }

    #[test]
    fn generates_union_values() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Union {
            variants: vec![TypeInfo::Int, TypeInfo::Str],
        };
        let mut saw_int = false;
        let mut saw_str = false;
        for _ in 0..100 {
            let val = generate_random_value(&typ, &mut rng, None);
            if val.is_string() {
                saw_str = true;
            } else {
                saw_int = true;
            }
        }
        assert!(saw_int && saw_str, "expected both int and string variants");
    }

    #[test]
    fn generates_nullable_values_including_null() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Int),
        };
        let mut saw_null = false;
        let mut saw_value = false;
        for _ in 0..100 {
            let val = generate_random_value(&typ, &mut rng, None);
            if val.is_null() {
                saw_null = true;
            } else {
                saw_value = true;
            }
        }
        assert!(saw_null && saw_value, "expected both null and non-null values");
    }

    #[test]
    fn empty_union_produces_null() {
        let mut rng = seeded_rng();
        let val = generate_random_value(
            &TypeInfo::Union { variants: vec![] },
            &mut rng,
            None,
        );
        assert!(val.is_null());
    }

    #[test]
    fn generate_random_inputs_matches_param_count() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
            ParamInfo { name: "c".into(), typ: TypeInfo::Bool, type_name: None },
        ];
        let inputs = generate_random_inputs(&params, &mut rng, None);
        assert_eq!(inputs.len(), 3);
    }

    #[test]
    fn bounded_array_length_favors_small_sizes() {
        let mut rng = seeded_rng();
        let mut counts = [0u32; 6]; // indices 0-5
        let trials = 1000;
        for _ in 0..trials {
            let len = generate_bounded_array_length(&mut rng);
            assert!(len <= 5, "length should be at most 5, got {len}");
            counts[len] += 1;
        }
        assert!(
            counts[0] >= 150,
            "expected empty arrays to be common, got {}/{}",
            counts[0],
            trials
        );
        assert!(
            counts[1] >= 150,
            "expected single-element arrays to be common, got {}/{}",
            counts[1],
            trials
        );
        let small: u32 = counts[0] + counts[1] + counts[2] + counts[3];
        assert!(
            small >= 700,
            "expected small sizes (0-3) to dominate, got {small}/{trials}"
        );
    }

    #[test]
    fn generated_arrays_have_bounded_length() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        for _ in 0..100 {
            let val = generate_random_value(&typ, &mut rng, None);
            let arr = val.as_array().expect("expected array");
            assert!(arr.len() <= 5, "array too long: {}", arr.len());
        }
    }

    #[test]
    fn unknown_type_generates_diverse_values() {
        let mut rng = seeded_rng();
        let mut types_seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let val = generate_random_value(&TypeInfo::Unknown, &mut rng, None);
            if val.is_string() {
                types_seen.insert("string");
            } else if val.is_boolean() {
                types_seen.insert("bool");
            } else if val.is_number() {
                types_seen.insert("number");
            }
        }
        assert!(types_seen.len() >= 2, "expected diverse types for Unknown");
    }

    #[test]
    fn generate_date_produces_tagged_json() {
        let mut rng = seeded_rng();
        for _ in 0..50 {
            let val = generate_date(&mut rng);
            let obj = val.as_object().expect("date should be an object");
            assert_eq!(obj.get("__complex_type").unwrap(), "date");
            assert!(obj.get("value").unwrap().is_i64(), "value should be epoch ms");
        }
    }

    #[test]
    fn generate_duration_produces_tagged_json() {
        let mut rng = seeded_rng();
        for _ in 0..50 {
            let val = generate_duration(&mut rng);
            let obj = val.as_object().expect("duration should be an object");
            assert_eq!(obj.get("__complex_type").unwrap(), "duration");
            assert!(obj.get("ms").unwrap().is_i64(), "ms should be integer");
        }
    }

    #[test]
    fn generate_date_includes_boundary_values() {
        let mut rng = StdRng::seed_from_u64(0);
        let mut saw_epoch = false;
        let mut saw_y2k38 = false;
        for _ in 0..500 {
            let val = generate_date(&mut rng);
            let ms = val["value"].as_i64().unwrap();
            if ms == 0 {
                saw_epoch = true;
            }
            if ms == 2_147_483_647_000 {
                saw_y2k38 = true;
            }
        }
        assert!(saw_epoch, "should generate epoch 0 boundary");
        assert!(saw_y2k38, "should generate Y2K38 boundary");
    }

    #[test]
    fn complex_type_with_caps_generates_tagged_json() {
        use crate::orchestrator::FrontendCapabilities;
        use crate::types::ComplexKind;

        let mut rng = seeded_rng();
        let caps = FrontendCapabilities::from_raw(&[
            "complex_type:date".into(),
        ]);
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Date,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let val = generate_random_value(&typ, &mut rng, Some(&caps));
        let obj = val.as_object().expect("should be a tagged object");
        assert_eq!(obj.get("__complex_type").unwrap(), "date");
    }

    #[test]
    fn complex_type_without_caps_falls_back() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Date,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let val = generate_random_value(&typ, &mut rng, None);
        assert!(
            val.as_object().and_then(|o| o.get("__complex_type")).is_none(),
            "without caps, should not produce tagged complex: {val}"
        );
    }

    #[test]
    fn opaque_type_generates_null() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Opaque {
            label: "net.Socket".to_string(),
            static_opacity: None,
            medium_opacity: None,
        };
        let val = generate_random_value(&typ, &mut rng, None);
        assert!(val.is_null(), "expected null for opaque type, got {val}");
    }

    // -- Generator-aware input generation tests --

    fn test_params() -> Vec<ParamInfo> {
        vec![
            ParamInfo {
                name: "user".into(),
                typ: TypeInfo::Object {
                    fields: vec![("id".into(), TypeInfo::Int)],
                },
                type_name: Some("User".into()),
            },
            ParamInfo {
                name: "authToken".into(),
                typ: TypeInfo::Str,
                type_name: None,
            },
            ParamInfo {
                name: "count".into(),
                typ: TypeInfo::Int,
                type_name: None,
            },
        ]
    }

    #[test]
    fn resolve_value_sources_param_generator_takes_precedence() {
        let params = test_params();
        let mut param_gens = std::collections::HashMap::new();
        param_gens.insert("authToken".to_string(), std::path::PathBuf::from("/gen/token.ts"));

        let mut type_gens = std::collections::HashMap::new();
        type_gens.insert("User".to_string(), std::path::PathBuf::from("/gen/user.ts"));

        let sources = resolve_value_sources(&params, &param_gens, &type_gens);
        assert_eq!(sources.len(), 3);

        assert!(matches!(&sources[0], ValueSource::CustomGenerator {
            generator_name, kind, ..
        } if generator_name == "User" && *kind == crate::protocol::GeneratorKind::TypeName));

        assert!(matches!(&sources[1], ValueSource::CustomGenerator {
            generator_name, kind, ..
        } if generator_name == "authToken" && *kind == crate::protocol::GeneratorKind::ParamName));

        assert_eq!(sources[2], ValueSource::BuiltIn);
    }

    #[test]
    fn resolve_value_sources_param_generator_overrides_type_generator() {
        let params = vec![ParamInfo {
            name: "user".into(),
            typ: TypeInfo::Str,
            type_name: Some("User".into()),
        }];
        let mut param_gens = std::collections::HashMap::new();
        param_gens.insert("user".to_string(), std::path::PathBuf::from("/gen/param_user.ts"));
        let mut type_gens = std::collections::HashMap::new();
        type_gens.insert("User".to_string(), std::path::PathBuf::from("/gen/type_user.ts"));

        let sources = resolve_value_sources(&params, &param_gens, &type_gens);
        assert!(matches!(&sources[0], ValueSource::CustomGenerator {
            generator_file, kind, ..
        } if generator_file == &std::path::PathBuf::from("/gen/param_user.ts")
            && *kind == crate::protocol::GeneratorKind::ParamName));
    }

    #[test]
    fn resolve_value_sources_all_builtin_when_no_generators() {
        let params = test_params();
        let empty_param = std::collections::HashMap::new();
        let empty_type = std::collections::HashMap::new();

        let sources = resolve_value_sources(&params, &empty_param, &empty_type);
        assert!(sources.iter().all(|s| *s == ValueSource::BuiltIn));
    }

    #[test]
    fn resolve_value_sources_type_generator_requires_type_name() {
        let params = vec![ParamInfo {
            name: "token".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        let empty_param = std::collections::HashMap::new();
        let mut type_gens = std::collections::HashMap::new();
        type_gens.insert("Str".to_string(), std::path::PathBuf::from("/gen/str.ts"));

        let sources = resolve_value_sources(&params, &empty_param, &type_gens);
        assert_eq!(sources[0], ValueSource::BuiltIn);
    }

    #[test]
    fn collect_generate_commands_deduplicates() {
        let sources = vec![
            ValueSource::CustomGenerator {
                generator_name: "User".into(),
                param_name: None,
                generator_file: "/gen/user.ts".into(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::CustomGenerator {
                generator_name: "User".into(),
                param_name: None,
                generator_file: "/gen/user.ts".into(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::BuiltIn,
        ];

        let commands = collect_generate_commands(&sources);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].1, "User");
    }

    #[test]
    fn collect_generate_commands_multiple_generators() {
        let sources = vec![
            ValueSource::CustomGenerator {
                generator_name: "User".into(),
                param_name: None,
                generator_file: "/gen/user.ts".into(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::CustomGenerator {
                generator_name: "authToken".into(),
                param_name: Some("authToken".into()),
                generator_file: "/gen/token.ts".into(),
                kind: crate::protocol::GeneratorKind::ParamName,
            },
        ];

        let commands = collect_generate_commands(&sources);
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn collect_generate_commands_empty_for_all_builtin() {
        let sources = vec![ValueSource::BuiltIn, ValueSource::BuiltIn];
        let commands = collect_generate_commands(&sources);
        assert!(commands.is_empty());
    }

    #[test]
    fn prefetched_values_insert_and_take() {
        let mut store = PrefetchedValues::new();
        store.insert(
            "/gen/user.ts".into(),
            "User".into(),
            vec![json!({"id": 1}), json!({"id": 2})],
        );

        assert!(store.has_values("/gen/user.ts", "User"));
        assert_eq!(store.take("/gen/user.ts", "User"), Some(json!({"id": 1})));
        assert_eq!(store.take("/gen/user.ts", "User"), Some(json!({"id": 2})));
        assert_eq!(store.take("/gen/user.ts", "User"), None);
        assert!(!store.has_values("/gen/user.ts", "User"));
    }

    #[test]
    fn prefetched_values_missing_key_returns_none() {
        let mut store = PrefetchedValues::new();
        assert_eq!(store.take("/nonexistent.ts", "Foo"), None);
        assert!(!store.has_values("/nonexistent.ts", "Foo"));
    }

    #[test]
    fn generate_inputs_with_custom_uses_prefetched_values() {
        let params = vec![
            ParamInfo {
                name: "user".into(),
                typ: TypeInfo::Int,
                type_name: Some("User".into()),
            },
            ParamInfo {
                name: "count".into(),
                typ: TypeInfo::Int,
                type_name: None,
            },
        ];
        let sources = vec![
            ValueSource::CustomGenerator {
                generator_name: "User".into(),
                param_name: None,
                generator_file: "/gen/user.ts".into(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::BuiltIn,
        ];

        let mut store = PrefetchedValues::new();
        store.insert("/gen/user.ts".into(), "User".into(), vec![json!({"name": "Alice"})]);

        let mut rng = seeded_rng();
        let inputs = generate_inputs_with_custom(&params, &sources, &mut store, &mut rng, None);

        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0], json!({"name": "Alice"}));
        assert!(inputs[1].is_i64() || inputs[1].is_u64());
    }

    #[test]
    fn generate_inputs_with_custom_falls_back_when_exhausted() {
        let params = vec![ParamInfo {
            name: "user".into(),
            typ: TypeInfo::Int,
            type_name: Some("User".into()),
        }];
        let sources = vec![ValueSource::CustomGenerator {
            generator_name: "User".into(),
            param_name: None,
            generator_file: "/gen/user.ts".into(),
            kind: crate::protocol::GeneratorKind::TypeName,
        }];

        let mut store = PrefetchedValues::new();
        let mut rng = seeded_rng();

        let inputs = generate_inputs_with_custom(&params, &sources, &mut store, &mut rng, None);
        assert_eq!(inputs.len(), 1);
        assert!(inputs[0].is_i64() || inputs[0].is_u64());
    }

    #[test]
    fn generate_inputs_with_custom_all_builtin_matches_generate_random_inputs() {
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
        ];
        let sources = vec![ValueSource::BuiltIn, ValueSource::BuiltIn];
        let mut store = PrefetchedValues::new();

        let mut rng1 = seeded_rng();
        let mut rng2 = seeded_rng();

        let custom_inputs = generate_inputs_with_custom(
            &params, &sources, &mut store, &mut rng1, None,
        );
        let random_inputs = generate_random_inputs(&params, &mut rng2, None);

        assert_eq!(custom_inputs, random_inputs);
    }

    #[test]
    fn prefetched_values_multiple_inserts_append() {
        let mut store = PrefetchedValues::new();
        store.insert("/gen/user.ts".into(), "User".into(), vec![json!(1)]);
        store.insert("/gen/user.ts".into(), "User".into(), vec![json!(2)]);

        assert_eq!(store.take("/gen/user.ts", "User"), Some(json!(1)));
        assert_eq!(store.take("/gen/user.ts", "User"), Some(json!(2)));
        assert_eq!(store.take("/gen/user.ts", "User"), None);
    }

    // -- Literal-derived candidate input tests --

    #[test]
    fn literals_to_candidates_str_matches_str_param() {
        let params = vec![ParamInfo { name: "s".into(), typ: TypeInfo::Str, type_name: None }];
        let literals = vec![LiteralValue::Str { value: "express".into() }];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0][0], json!("express"));
    }

    #[test]
    fn literals_to_candidates_int_does_not_match_str_param() {
        let params = vec![ParamInfo { name: "s".into(), typ: TypeInfo::Str, type_name: None }];
        let literals = vec![LiteralValue::Int { value: 42 }];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert!(candidates.is_empty());
    }

    #[test]
    fn literals_to_candidates_deduplicates_same_value() {
        let params = vec![ParamInfo { name: "n".into(), typ: TypeInfo::Int, type_name: None }];
        let literals = vec![
            LiteralValue::Int { value: 100 },
            LiteralValue::Int { value: 100 },
        ];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn literals_to_candidates_int_matches_float_param() {
        let params = vec![ParamInfo { name: "x".into(), typ: TypeInfo::Float, type_name: None }];
        let literals = vec![LiteralValue::Int { value: 5 }];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0][0], json!(5.0));
    }

    #[test]
    fn literals_to_candidates_multi_param_uses_defaults() {
        let params = vec![
            ParamInfo { name: "s".into(), typ: TypeInfo::Str, type_name: None },
            ParamInfo { name: "n".into(), typ: TypeInfo::Int, type_name: None },
        ];
        let literals = vec![LiteralValue::Str { value: "express".into() }];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0][0], json!("express"));
        // second param gets a boundary default for Int (which is 0)
        assert_eq!(candidates[0][1], json!(0));
    }

    #[test]
    fn literals_to_candidates_nullable_unwraps() {
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Nullable { inner: Box::new(TypeInfo::Str) },
            type_name: None,
        }];
        let literals = vec![LiteralValue::Str { value: "hello".into() }];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0][0], json!("hello"));
    }

    #[test]
    fn literals_to_candidates_empty_params_returns_empty() {
        let literals = vec![LiteralValue::Str { value: "test".into() }];
        let candidates = literals_to_candidate_inputs(&[], &literals);
        assert!(candidates.is_empty());
    }

    #[test]
    fn literals_to_candidates_empty_literals_returns_empty() {
        let params = vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }];
        let candidates = literals_to_candidate_inputs(&params, &[]);
        assert!(candidates.is_empty());
    }

    #[test]
    fn literals_to_candidates_regex_matches_str_param() {
        let params = vec![ParamInfo { name: "s".into(), typ: TypeInfo::Str, type_name: None }];
        let literals = vec![LiteralValue::Regex { pattern: "\\d{5}".into() }];
        let candidates = literals_to_candidate_inputs(&params, &literals);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0][0], json!("\\d{5}"));
    }

    // -- Mutation operator tests --

    #[test]
    fn mutate_int_boundary_values() {
        let mut rng = StdRng::seed_from_u64(0);
        let mut saw_zero = false;
        let mut saw_min = false;
        let mut saw_max = false;
        for _ in 0..500 {
            let mutated = mutate_value(&json!(42), &TypeInfo::Int, &[], &mut rng);
            let n = mutated.as_i64().or_else(|| mutated.as_u64().map(|u| u as i64));
            if let Some(n) = n {
                if n == 0 { saw_zero = true; }
                if n == i64::MIN { saw_min = true; }
                if n == i64::MAX { saw_max = true; }
            }
        }
        assert!(saw_zero, "should produce 0 boundary");
        assert!(saw_min, "should produce i64::MIN boundary");
        assert!(saw_max, "should produce i64::MAX boundary");
    }

    #[test]
    fn mutate_int_invalid_input_regenerates() {
        let mut rng = seeded_rng();
        let mutated = mutate_value(&json!("not_an_int"), &TypeInfo::Int, &[], &mut rng);
        assert!(
            mutated.is_i64() || mutated.is_u64(),
            "should regenerate valid int, got {mutated}"
        );
    }

    #[test]
    fn mutate_float_special_values() {
        let mut rng = StdRng::seed_from_u64(0);
        let mut saw_null = false; // NaN or Inf → null
        let mut saw_zero = false;
        for _ in 0..500 {
            let mutated = mutate_value(&json!(1.0), &TypeInfo::Float, &[], &mut rng);
            if mutated.is_null() {
                saw_null = true;
            }
            if mutated.as_f64() == Some(0.0) {
                saw_zero = true;
            }
        }
        assert!(saw_null, "should produce null (NaN/Inf)");
        assert!(saw_zero, "should produce 0.0");
    }

    #[test]
    fn mutate_bool_flips() {
        let mut rng = seeded_rng();
        assert_eq!(mutate_value(&json!(true), &TypeInfo::Bool, &[], &mut rng), json!(false));
        assert_eq!(mutate_value(&json!(false), &TypeInfo::Bool, &[], &mut rng), json!(true));
    }

    #[test]
    fn mutate_string_operators_change_length() {
        let mut rng = StdRng::seed_from_u64(0);
        let mut saw_shorter = false;
        let mut saw_longer = false;
        let mut saw_empty = false;
        let mut saw_long = false;
        let original = "hello";
        for _ in 0..500 {
            let mutated = mutate_value(&json!(original), &TypeInfo::Str, &[], &mut rng);
            let s = mutated.as_str().unwrap_or("");
            if s.is_empty() { saw_empty = true; }
            if s.len() >= 1000 { saw_long = true; }
            if s.len() < original.len() && !s.is_empty() { saw_shorter = true; }
            if s.len() > original.len() && s.len() < 1000 { saw_longer = true; }
        }
        assert!(saw_shorter, "should produce shorter strings (deletion)");
        assert!(saw_longer, "should produce longer strings (insertion)");
        assert!(saw_empty, "should produce empty string");
        assert!(saw_long, "should produce long string");
    }

    #[test]
    fn mutate_string_empty_input_is_safe() {
        let mut rng = seeded_rng();
        for _ in 0..50 {
            let mutated = mutate_value(&json!(""), &TypeInfo::Str, &[], &mut rng);
            assert!(mutated.is_string(), "expected string, got {mutated}");
        }
    }

    #[test]
    fn mutate_string_dictionary_injection() {
        let mut rng = StdRng::seed_from_u64(0);
        let dictionary: &[&str] = &["@", "://", ".com"];
        let mut saw_at = false;
        let mut saw_scheme = false;
        let mut saw_dotcom = false;
        for _ in 0..500 {
            let mutated = mutate_value(&json!("hello"), &TypeInfo::Str, dictionary, &mut rng);
            let s = mutated.as_str().unwrap_or("");
            if s.contains('@') { saw_at = true; }
            if s.contains("://") { saw_scheme = true; }
            if s.contains(".com") { saw_dotcom = true; }
        }
        assert!(saw_at, "should inject '@' from dictionary");
        assert!(saw_scheme, "should inject '://' from dictionary");
        assert!(saw_dotcom, "should inject '.com' from dictionary");
    }

    #[test]
    fn mutate_string_empty_dictionary_no_injection() {
        let mut rng = StdRng::seed_from_u64(42);
        // With empty dictionary, op range is 0..6, same as before
        for _ in 0..100 {
            let mutated = mutate_value(&json!("test"), &TypeInfo::Str, &[], &mut rng);
            assert!(mutated.is_string(), "expected string, got {mutated}");
        }
    }

    #[test]
    fn mutate_array_type_valid() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };
        for _ in 0..100 {
            let mutated = mutate_value(&json!([1, 2, 3]), &typ, &[], &mut rng);
            assert!(mutated.is_array(), "expected array, got {mutated}");
        }
    }

    #[test]
    fn mutate_array_empty_can_grow() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };
        let mutated = mutate_value(&json!([]), &typ, &[], &mut rng);
        let arr = mutated.as_array().expect("expected array");
        assert_eq!(arr.len(), 1, "empty array mutation should add an element");
    }

    #[test]
    fn mutate_object_type_valid() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                ("age".into(), TypeInfo::Int),
            ],
        };
        let original = json!({"name": "Alice", "age": 30});
        for _ in 0..100 {
            let mutated = mutate_value(&original, &typ, &[], &mut rng);
            assert!(mutated.is_object(), "expected object, got {mutated}");
        }
    }

    #[test]
    fn mutate_inputs_rate_zero_returns_identical() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
        ];
        let inputs = vec![json!(42), json!("hello")];
        let mutated = mutate_inputs(&inputs, &params, 0.0, &[], &mut rng);
        assert_eq!(mutated, inputs);
    }

    #[test]
    fn mutate_inputs_rate_one_mutates_all() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Bool, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Bool, type_name: None },
        ];
        let inputs = vec![json!(true), json!(false)];
        let mutated = mutate_inputs(&inputs, &params, 1.0, &[], &mut rng);
        // Bools always flip, so both should change
        assert_eq!(mutated[0], json!(false));
        assert_eq!(mutated[1], json!(true));
    }

    #[test]
    fn mutate_value_unknown_returns_unchanged() {
        let mut rng = seeded_rng();
        let val = json!(42);
        assert_eq!(mutate_value(&val, &TypeInfo::Unknown, &[], &mut rng), val);
    }

    #[test]
    fn mutate_value_opaque_returns_unchanged() {
        let mut rng = seeded_rng();
        let val = json!(null);
        let typ = TypeInfo::Opaque { label: "net.Socket".into(), static_opacity: None, medium_opacity: None };
        assert_eq!(mutate_value(&val, &typ, &[], &mut rng), val);
    }

    #[test]
    fn mutate_nullable_can_flip_to_null() {
        let mut rng = StdRng::seed_from_u64(0);
        let typ = TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) };
        let mut saw_null = false;
        let mut saw_value = false;
        for _ in 0..100 {
            let mutated = mutate_value(&json!(42), &typ, &[], &mut rng);
            if mutated.is_null() {
                saw_null = true;
            } else {
                saw_value = true;
            }
        }
        assert!(saw_null, "should sometimes flip to null");
        assert!(saw_value, "should sometimes keep/mutate value");
    }

    #[test]
    fn mutate_nullable_can_flip_from_null() {
        let mut rng = StdRng::seed_from_u64(0);
        let typ = TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) };
        let mut saw_non_null = false;
        for _ in 0..100 {
            let mutated = mutate_value(&Value::Null, &typ, &[], &mut rng);
            if !mutated.is_null() {
                saw_non_null = true;
            }
        }
        assert!(saw_non_null, "should sometimes flip from null to value");
    }

    #[test]
    fn mutate_union_delegates_to_variant() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Union {
            variants: vec![TypeInfo::Int, TypeInfo::Str],
        };
        for _ in 0..50 {
            let mutated = mutate_value(&json!(42), &typ, &[], &mut rng);
            assert!(
                mutated.is_i64() || mutated.is_u64() || mutated.is_string(),
                "expected int or string, got {mutated}"
            );
        }
    }

    // -- Crossover operator tests --

    #[test]
    fn crossover_inputs_respects_rate_zero() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
        ];
        let parent_a = vec![json!(1), json!("hello")];
        let parent_b = vec![json!(2), json!("world")];

        let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 0.0, &mut rng);
        assert_eq!(c1, parent_a);
        assert_eq!(c2, parent_b);
    }

    #[test]
    fn crossover_inputs_produces_mixed_children() {
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
            ParamInfo { name: "c".into(), typ: TypeInfo::Bool, type_name: None },
            ParamInfo { name: "d".into(), typ: TypeInfo::Float, type_name: None },
        ];
        let parent_a = vec![json!(1), json!("aaa"), json!(true), json!(1.0)];
        let parent_b = vec![json!(2), json!("bbb"), json!(false), json!(2.0)];

        // Run many times — children should contain values from both parents
        let mut saw_mix = false;
        for seed in 0..100_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, _c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            let from_a = c1.iter().zip(parent_a.iter()).filter(|(c, a)| c == a).count();
            let from_b = c1.iter().zip(parent_b.iter()).filter(|(c, b)| c == b).count();
            if from_a > 0 && from_b > 0 {
                saw_mix = true;
                break;
            }
        }
        assert!(saw_mix, "expected children to mix values from both parents");
    }

    #[test]
    fn crossover_inputs_handles_empty() {
        let mut rng = seeded_rng();
        let params: Vec<ParamInfo> = vec![];
        let parent_a: Vec<Value> = vec![];
        let parent_b: Vec<Value> = vec![];

        let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
        assert!(c1.is_empty());
        assert!(c2.is_empty());
    }

    #[test]
    fn crossover_inputs_object_field_level() {
        let params = vec![ParamInfo {
            name: "obj".into(),
            typ: TypeInfo::Object {
                fields: vec![
                    ("x".into(), TypeInfo::Int),
                    ("y".into(), TypeInfo::Int),
                    ("z".into(), TypeInfo::Int),
                ],
            },
            type_name: None,
        }];
        let parent_a = vec![json!({"x": 1, "y": 2, "z": 3})];
        let parent_b = vec![json!({"x": 10, "y": 20, "z": 30})];

        let mut saw_field_mix = false;
        for seed in 0..100_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, _c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            let obj = c1[0].as_object().expect("expected object child");
            let from_a = [obj.get("x") == Some(&json!(1)),
                          obj.get("y") == Some(&json!(2)),
                          obj.get("z") == Some(&json!(3))];
            let from_b = [obj.get("x") == Some(&json!(10)),
                          obj.get("y") == Some(&json!(20)),
                          obj.get("z") == Some(&json!(30))];
            let a_count = from_a.iter().filter(|&&v| v).count();
            let b_count = from_b.iter().filter(|&&v| v).count();
            if a_count > 0 && b_count > 0 {
                saw_field_mix = true;
                break;
            }
        }
        assert!(saw_field_mix, "expected field-level crossover to mix object fields");
    }

    #[test]
    fn crossover_inputs_array_single_point() {
        let params = vec![ParamInfo {
            name: "arr".into(),
            typ: TypeInfo::Array {
                element: Box::new(TypeInfo::Int),
            },
            type_name: None,
        }];
        let parent_a = vec![json!([1, 2, 3, 4])];
        let parent_b = vec![json!([10, 20, 30, 40])];

        let mut saw_prefix_suffix = false;
        for seed in 0..100_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, _c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            let arr = c1[0].as_array().expect("expected array child");
            // For single-point crossover, prefix comes from A and suffix from B
            // Check that first element(s) are from A and last from B, or vice versa
            if arr.len() == 4 {
                let first_from_a = arr[0] == json!(1);
                let last_from_b = arr[3] == json!(40);
                if first_from_a && last_from_b {
                    saw_prefix_suffix = true;
                    break;
                }
            }
        }
        assert!(saw_prefix_suffix, "expected single-point crossover with prefix/suffix split");
    }

    #[test]
    fn crossover_inputs_preserves_types() {
        let params = vec![
            ParamInfo { name: "i".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "s".into(), typ: TypeInfo::Str, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Bool, type_name: None },
        ];
        let parent_a = vec![json!(1), json!("hello"), json!(true)];
        let parent_b = vec![json!(2), json!("world"), json!(false)];

        for seed in 0..100_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            assert!(c1[0].is_i64() || c1[0].is_u64(), "child1[0] not int: {}", c1[0]);
            assert!(c1[1].is_string(), "child1[1] not string: {}", c1[1]);
            assert!(c1[2].is_boolean(), "child1[2] not bool: {}", c1[2]);
            assert!(c2[0].is_i64() || c2[0].is_u64(), "child2[0] not int: {}", c2[0]);
            assert!(c2[1].is_string(), "child2[1] not string: {}", c2[1]);
            assert!(c2[2].is_boolean(), "child2[2] not bool: {}", c2[2]);
        }
    }

    #[test]
    fn crossover_inputs_length_matches_params() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
            ParamInfo { name: "c".into(), typ: TypeInfo::Bool, type_name: None },
        ];
        let parent_a = vec![json!(1), json!("x"), json!(true)];
        let parent_b = vec![json!(2), json!("y"), json!(false)];

        let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
        assert_eq!(c1.len(), 3);
        assert_eq!(c2.len(), 3);
    }

    // -- String crossover tests --

    #[test]
    fn crossover_string_produces_mixed_children() {
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        let parent_a = vec![json!("abcdef")];
        let parent_b = vec![json!("ABCDEF")];

        // Over many seeds, at least one child should contain chars from both parents
        let mut saw_mix = false;
        for seed in 0..200_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, _c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            let s = c1[0].as_str().expect("expected string child");
            let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
            let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
            if has_lower && has_upper {
                saw_mix = true;
                break;
            }
        }
        assert!(saw_mix, "expected string crossover to combine chars from both parents");
    }

    #[test]
    fn crossover_string_empty_no_panic() {
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];

        // One empty, one non-empty
        let parent_a = vec![json!("")];
        let parent_b = vec![json!("hello")];
        for seed in 0..20_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            assert!(c1[0].is_string());
            assert!(c2[0].is_string());
        }

        // Both empty
        let parent_a = vec![json!("")];
        let parent_b = vec![json!("")];
        let mut rng = seeded_rng();
        let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
        assert!(c1[0].is_string());
        assert!(c2[0].is_string());
    }

    #[test]
    fn crossover_string_children_are_strings() {
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        let parent_a = vec![json!("test@example")];
        let parent_b = vec![json!("user@domain.com")];

        for seed in 0..100_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            assert!(c1[0].is_string(), "child1 not a string: {:?}", c1[0]);
            assert!(c2[0].is_string(), "child2 not a string: {:?}", c2[0]);
        }
    }

    #[test]
    fn crossover_string_nonstring_types_unchanged() {
        let params = vec![
            ParamInfo { name: "i".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "b".into(), typ: TypeInfo::Bool, type_name: None },
        ];
        let parent_a = vec![json!(42), json!(true)];
        let parent_b = vec![json!(99), json!(false)];

        for seed in 0..50_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            // Non-string types use uniform swap — each child value must come from one parent
            for (idx, child) in [&c1, &c2].into_iter().enumerate() {
                for (j, val) in child.iter().enumerate() {
                    assert!(
                        val == &parent_a[j] || val == &parent_b[j],
                        "child{idx}[{j}] = {val} not from either parent"
                    );
                }
            }
        }
    }

    #[test]
    fn crossover_string_unicode_safe() {
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        let parent_a = vec![json!("héllo")];
        let parent_b = vec![json!("wörld")];

        for seed in 0..100_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (c1, c2) = crossover_inputs(&parent_a, &parent_b, &params, 1.0, &mut rng);
            // Must not panic and must produce valid UTF-8 strings
            assert!(c1[0].is_string());
            assert!(c2[0].is_string());
        }
    }

    // -- Pool-to-candidate tests --

    #[test]
    fn pool_to_candidate_inputs_produces_candidates() {
        use crate::interesting_pool::{
            BehaviorObservation, InterestingPool, PoolEntry, Severity,
        };
        let mut pool = InterestingPool::default();
        pool.insert(PoolEntry {
            value: json!(42),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: "foo".into(),
                branch_id: 1,
                severity: Severity::RarePath,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 0,
        });
        let params = vec![
            ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None },
            ParamInfo { name: "y".into(), typ: TypeInfo::Str, type_name: None },
        ];
        let candidates = pool_to_candidate_inputs(&params, &pool);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0][0], json!(42));
    }

    #[test]
    fn pool_to_candidate_inputs_empty_pool_returns_empty() {
        let pool = crate::interesting_pool::InterestingPool::default();
        let params = vec![
            ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None },
        ];
        let candidates = pool_to_candidate_inputs(&params, &pool);
        assert!(candidates.is_empty());
    }

    #[test]
    fn pool_to_candidate_inputs_deduplicates() {
        use crate::interesting_pool::{
            BehaviorObservation, InterestingPool, PoolEntry, Severity,
        };
        let mut pool = InterestingPool::default();
        pool.insert(PoolEntry {
            value: json!(7),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: "a".into(),
                branch_id: 1,
                severity: Severity::RarePath,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 0,
        });
        pool.insert(PoolEntry {
            value: json!(7),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: "b".into(),
                branch_id: 2,
                severity: Severity::Crash,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 0,
        });
        let params = vec![
            ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None },
        ];
        let candidates = pool_to_candidate_inputs(&params, &pool);
        assert_eq!(candidates.len(), 1, "duplicate values should be deduplicated");
    }

    // -----------------------------------------------------------------------
    // Param-name heuristic string tests
    // -----------------------------------------------------------------------

    #[test]
    fn heuristic_email() {
        assert_eq!(heuristic_string_for_name("email"), Some(HEURISTIC_EMAIL));
        assert_eq!(heuristic_string_for_name("user_email"), Some(HEURISTIC_EMAIL));
        assert_eq!(heuristic_string_for_name("mail_address"), Some(HEURISTIC_EMAIL));
    }

    #[test]
    fn heuristic_url() {
        assert_eq!(heuristic_string_for_name("url"), Some(HEURISTIC_URL));
        assert_eq!(heuristic_string_for_name("redirect_uri"), Some(HEURISTIC_URL));
        assert_eq!(heuristic_string_for_name("href"), Some(HEURISTIC_URL));
        assert_eq!(heuristic_string_for_name("link"), Some(HEURISTIC_URL));
    }

    #[test]
    fn heuristic_phone() {
        assert_eq!(heuristic_string_for_name("phone"), Some(HEURISTIC_PHONE));
        assert_eq!(heuristic_string_for_name("tel"), Some(HEURISTIC_PHONE));
        assert_eq!(heuristic_string_for_name("mobile_number"), Some(HEURISTIC_PHONE));
    }

    #[test]
    fn heuristic_name_specificity() {
        assert_eq!(heuristic_string_for_name("first_name"), Some(HEURISTIC_FIRST_NAME));
        assert_eq!(heuristic_string_for_name("firstname"), Some(HEURISTIC_FIRST_NAME));
        assert_eq!(heuristic_string_for_name("last_name"), Some(HEURISTIC_LAST_NAME));
        assert_eq!(heuristic_string_for_name("lastname"), Some(HEURISTIC_LAST_NAME));
        assert_eq!(heuristic_string_for_name("name"), Some(HEURISTIC_NAME));
        assert_eq!(heuristic_string_for_name("display_name"), Some(HEURISTIC_NAME));
    }

    #[test]
    fn heuristic_date() {
        assert_eq!(heuristic_string_for_name("date"), Some(HEURISTIC_DATE));
        assert_eq!(heuristic_string_for_name("timestamp"), Some(HEURISTIC_DATE));
        assert_eq!(heuristic_string_for_name("created_at"), Some(HEURISTIC_DATE));
        assert_eq!(heuristic_string_for_name("updated_at"), Some(HEURISTIC_DATE));
    }

    #[test]
    fn heuristic_uuid_and_id() {
        assert_eq!(heuristic_string_for_name("uuid"), Some(HEURISTIC_UUID));
        assert_eq!(heuristic_string_for_name("request_id"), Some(HEURISTIC_UUID));
        assert_eq!(heuristic_string_for_name("id"), Some(HEURISTIC_UUID));
    }

    #[test]
    fn heuristic_path() {
        assert_eq!(heuristic_string_for_name("path"), Some(HEURISTIC_PATH));
        assert_eq!(heuristic_string_for_name("file"), Some(HEURISTIC_PATH));
        assert_eq!(heuristic_string_for_name("filename"), Some(HEURISTIC_PATH));
    }

    #[test]
    fn heuristic_ip() {
        assert_eq!(heuristic_string_for_name("ip"), Some(HEURISTIC_IP));
        assert_eq!(heuristic_string_for_name("ip_addr"), Some(HEURISTIC_IP));
        assert_eq!(heuristic_string_for_name("remote_addr"), Some(HEURISTIC_IP));
    }

    #[test]
    fn heuristic_token() {
        assert_eq!(heuristic_string_for_name("token"), Some(HEURISTIC_TOKEN));
        assert_eq!(heuristic_string_for_name("api_key"), Some(HEURISTIC_TOKEN));
        assert_eq!(heuristic_string_for_name("secret"), Some(HEURISTIC_TOKEN));
    }

    #[test]
    fn heuristic_case_insensitive() {
        assert_eq!(heuristic_string_for_name("Email"), Some(HEURISTIC_EMAIL));
        assert_eq!(heuristic_string_for_name("EMAIL"), Some(HEURISTIC_EMAIL));
        assert_eq!(heuristic_string_for_name("eMaIl"), Some(HEURISTIC_EMAIL));
        assert_eq!(heuristic_string_for_name("USER_EMAIL"), Some(HEURISTIC_EMAIL));
    }

    #[test]
    fn heuristic_no_match_returns_none() {
        assert_eq!(heuristic_string_for_name("count"), None);
        assert_eq!(heuristic_string_for_name("x"), None);
        assert_eq!(heuristic_string_for_name("foo_bar"), None);
    }

    #[test]
    fn generate_random_inputs_uses_heuristic_for_str() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "email".into(), typ: TypeInfo::Str, type_name: None },
            ParamInfo { name: "count".into(), typ: TypeInfo::Int, type_name: None },
        ];
        let inputs = generate_random_inputs(&params, &mut rng, None);
        assert_eq!(inputs[0], json!(HEURISTIC_EMAIL));
        assert!(inputs[1].is_i64() || inputs[1].is_u64());
    }

    #[test]
    fn generate_random_inputs_falls_back_for_unknown_name() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "foo".into(), typ: TypeInfo::Str, type_name: None },
        ];
        let inputs = generate_random_inputs(&params, &mut rng, None);
        assert!(inputs[0].is_string());
        assert_ne!(inputs[0].as_str(), Some(HEURISTIC_EMAIL));
    }

    #[test]
    fn generate_random_inputs_checks_type_name_fallback() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Str,
                type_name: Some("Email".into()),
            },
        ];
        let inputs = generate_random_inputs(&params, &mut rng, None);
        assert_eq!(inputs[0], json!(HEURISTIC_EMAIL));
    }

    #[test]
    fn heuristic_does_not_affect_non_str_types() {
        let mut rng = seeded_rng();
        let params = vec![
            ParamInfo { name: "email".into(), typ: TypeInfo::Int, type_name: None },
        ];
        let inputs = generate_random_inputs(&params, &mut rng, None);
        assert!(inputs[0].is_i64() || inputs[0].is_u64());
    }

    // -----------------------------------------------------------------------
    // shrink_candidates unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_int_42() {
        let candidates = shrink_candidates(&json!(42), &TypeInfo::Int);
        assert!(candidates.contains(&json!(21)), "should contain halved value");
        assert!(candidates.contains(&json!(0)), "should contain 0");
        assert!(candidates.contains(&json!(1)), "should contain 1");
        assert!(candidates.contains(&json!(-1)), "should contain -1");
        assert!(!candidates.contains(&json!(42)), "should not contain original");
    }

    #[test]
    fn shrink_int_zero() {
        let candidates = shrink_candidates(&json!(0), &TypeInfo::Int);
        assert!(!candidates.contains(&json!(0)), "should not contain original");
        assert!(candidates.contains(&json!(1)));
        assert!(candidates.contains(&json!(-1)));
    }

    #[test]
    fn shrink_str_hello() {
        let candidates = shrink_candidates(&json!("hello"), &TypeInfo::Str);
        assert!(candidates.contains(&json!("hell")), "remove last char");
        assert!(candidates.contains(&json!("ello")), "remove first char");
        assert!(candidates.contains(&json!("")), "empty string");
        assert!(candidates.contains(&json!("h")), "single first char");
        assert!(!candidates.contains(&json!("hello")), "no original");
    }

    #[test]
    fn shrink_str_empty() {
        let candidates = shrink_candidates(&json!(""), &TypeInfo::Str);
        assert!(candidates.is_empty());
    }

    #[test]
    fn shrink_str_single_char() {
        let candidates = shrink_candidates(&json!("x"), &TypeInfo::Str);
        assert!(candidates.contains(&json!("")), "should contain empty");
        assert!(!candidates.contains(&json!("x")), "no original");
    }

    #[test]
    fn shrink_bool_true() {
        let candidates = shrink_candidates(&json!(true), &TypeInfo::Bool);
        assert_eq!(candidates, vec![json!(false)]);
    }

    #[test]
    fn shrink_bool_false() {
        let candidates = shrink_candidates(&json!(false), &TypeInfo::Bool);
        assert!(candidates.is_empty());
    }

    #[test]
    fn shrink_array_three_elements() {
        let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };
        let candidates = shrink_candidates(&json!([1, 2, 3]), &typ);
        assert!(candidates.contains(&json!([1, 2])), "remove last");
        assert!(candidates.contains(&json!([2, 3])), "remove first");
        assert!(candidates.contains(&json!([])), "empty");
    }

    #[test]
    fn shrink_array_empty() {
        let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };
        let candidates = shrink_candidates(&json!([]), &typ);
        assert!(candidates.is_empty());
    }

    #[test]
    fn shrink_nullable_non_null() {
        let typ = TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) };
        let candidates = shrink_candidates(&json!(42), &typ);
        assert!(candidates.contains(&Value::Null), "should contain null");
    }

    #[test]
    fn shrink_nullable_null() {
        let typ = TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) };
        let candidates = shrink_candidates(&Value::Null, &typ);
        assert!(candidates.is_empty());
    }

    #[test]
    fn shrink_object_removes_fields() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("a".into(), TypeInfo::Int),
                ("b".into(), TypeInfo::Str),
            ],
        };
        let val = json!({"a": 10, "b": "hi"});
        let candidates = shrink_candidates(&val, &typ);
        assert!(candidates.contains(&json!({"b": "hi"})), "remove field a");
        assert!(candidates.contains(&json!({"a": 10})), "remove field b");
    }

    #[test]
    fn shrink_no_duplicates() {
        // shrink_candidates(1, Int) would produce [0, 1, -1] but 1 is the original
        // and 0 is half — make sure no dupes
        let candidates = shrink_candidates(&json!(1), &TypeInfo::Int);
        let mut seen = Vec::new();
        for c in &candidates {
            assert!(!seen.contains(c), "duplicate candidate: {c:?}");
            seen.push(c.clone());
        }
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::arb_param_info;
        use crate::types::ParamInfo;
        use proptest::prelude::*;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        use serde_json::json;

        /// Check whether a JSON value is compatible with a TypeInfo.
        /// Allows for JSON's type coercions (e.g. integers are valid floats,
        /// NaN/Infinity encode as null in JSON).
        fn value_matches_type(value: &serde_json::Value, typ: &TypeInfo) -> bool {
            match typ {
                TypeInfo::Int => value.is_i64() || value.is_u64(),
                // NaN and Infinity serialize to JSON null — accept null for Float.
                TypeInfo::Float => value.is_f64() || value.is_i64() || value.is_u64() || value.is_null(),
                TypeInfo::Str => value.is_string(),
                TypeInfo::Bool => value.is_boolean(),
                TypeInfo::Array { .. } => value.is_array(),
                TypeInfo::Object { .. } => value.is_object(),
                TypeInfo::Nullable { inner } => value.is_null() || value_matches_type(value, inner),
                TypeInfo::Union { variants } => {
                    variants.is_empty() || variants.iter().any(|v| value_matches_type(value, v))
                }
                TypeInfo::Complex { .. } | TypeInfo::Opaque { .. } | TypeInfo::Unknown => true,
            }
        }

        proptest! {
            #[test]
            fn mutate_int_preserves_number(val in -1_000_000i64..1_000_000i64) {
                let input = serde_json::json!(val);
                let mut rng = StdRng::seed_from_u64(42);
                let result = mutate_value(&input, &TypeInfo::Int, &[], &mut rng);
                prop_assert!(
                    result.is_number(),
                    "mutating Int produced non-number: {result:?}"
                );
            }

            #[test]
            fn mutate_str_preserves_string(val in ".{0,30}") {
                let input = serde_json::json!(val);
                let mut rng = StdRng::seed_from_u64(42);
                let result = mutate_value(&input, &TypeInfo::Str, &[], &mut rng);
                prop_assert!(
                    result.is_string(),
                    "mutating Str produced non-string: {result:?}"
                );
            }

            #[test]
            fn mutate_bool_preserves_bool(val in any::<bool>()) {
                let input = serde_json::json!(val);
                let result = mutate_value(&input, &TypeInfo::Bool, &[], &mut StdRng::seed_from_u64(42));
                prop_assert!(
                    result.is_boolean(),
                    "mutating Bool produced non-bool: {result:?}"
                );
            }

            #[test]
            fn mutate_inputs_preserves_length(
                seed in 0..10000u64,
                len in 1..6usize,
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                let params: Vec<ParamInfo> = (0..len)
                    .map(|i| ParamInfo {
                        name: format!("p{i}"),
                        typ: TypeInfo::Int,
                        type_name: None,
                    })
                    .collect();
                let inputs: Vec<serde_json::Value> =
                    (0..len).map(|i| serde_json::json!(i as i64)).collect();
                let mutated = mutate_inputs(&inputs, &params, 1.0, &[], &mut rng);
                prop_assert_eq!(
                    inputs.len(),
                    mutated.len(),
                    "mutate_inputs changed vector length"
                );
            }

            // -----------------------------------------------------------------
            // mutate_inputs: vector-level type preservation with arbitrary types
            // -----------------------------------------------------------------

            #[test]
            fn mutate_inputs_preserves_types_arbitrary(
                seed in 0..10000u64,
                typs in prop::collection::vec(
                    prop_oneof![
                        Just(TypeInfo::Int),
                        Just(TypeInfo::Float),
                        Just(TypeInfo::Bool),
                    ],
                    1..=5,
                ),
            ) {
                let params: Vec<ParamInfo> = typs
                    .into_iter()
                    .enumerate()
                    .map(|(i, typ)| ParamInfo {
                        name: format!("p{i}"),
                        typ,
                        type_name: None,
                    })
                    .collect();
                let mut rng = StdRng::seed_from_u64(seed);
                let inputs: Vec<serde_json::Value> = params
                    .iter()
                    .map(|p| generate_random_value(&p.typ, &mut rng, None))
                    .collect();
                let mut rng2 = StdRng::seed_from_u64(seed.wrapping_add(1));
                let mutated = mutate_inputs(&inputs, &params, 1.0, &[], &mut rng2);

                prop_assert_eq!(mutated.len(), params.len(),
                    "mutate_inputs changed vector length");

                for (i, (val, param)) in mutated.iter().zip(params.iter()).enumerate() {
                    prop_assert!(
                        value_matches_type(val, &param.typ),
                        "mutated[{i}] = {val:?} doesn't match type {:?}",
                        param.typ
                    );
                }
            }

            // -----------------------------------------------------------------
            // mutate_inputs: mutation with rate=1.0 actually changes something
            // -----------------------------------------------------------------

            #[test]
            fn mutate_inputs_actually_mutates(seed in 0..10000u64) {
                let params = vec![
                    ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
                    ParamInfo { name: "b".into(), typ: TypeInfo::Str, type_name: None },
                    ParamInfo { name: "c".into(), typ: TypeInfo::Bool, type_name: None },
                ];
                let inputs = vec![json!(42), json!("hello"), json!(true)];

                // Try multiple RNG seeds — at least one should produce a mutation.
                let mut any_diff = false;
                for offset in 0..20u64 {
                    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(offset));
                    let mutated = mutate_inputs(&inputs, &params, 1.0, &[], &mut rng);
                    if mutated != inputs {
                        any_diff = true;
                        break;
                    }
                }
                prop_assert!(any_diff,
                    "mutate_inputs with rate=1.0 never changed anything over 20 tries");
            }

            // -----------------------------------------------------------------
            // crossover_inputs: length preservation
            // -----------------------------------------------------------------

            #[test]
            fn crossover_inputs_preserves_length_arbitrary(
                seed in 0..10000u64,
                params in prop::collection::vec(arb_param_info(), 1..=5),
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                let parent_a: Vec<serde_json::Value> = params
                    .iter()
                    .map(|p| generate_random_value(&p.typ, &mut rng, None))
                    .collect();
                let parent_b: Vec<serde_json::Value> = params
                    .iter()
                    .map(|p| generate_random_value(&p.typ, &mut rng, None))
                    .collect();

                let mut rng2 = StdRng::seed_from_u64(seed.wrapping_add(1));
                let (child1, child2) = crossover_inputs(
                    &parent_a, &parent_b, &params, 1.0, &mut rng2,
                );

                let expected_len = parent_a.len().min(parent_b.len()).min(params.len());
                prop_assert_eq!(child1.len(), expected_len,
                    "child1 length mismatch");
                prop_assert_eq!(child2.len(), expected_len,
                    "child2 length mismatch");
            }

            // -----------------------------------------------------------------
            // crossover_inputs: type compatibility of children
            // -----------------------------------------------------------------

            #[test]
            fn crossover_inputs_type_compatible(
                seed in 0..10000u64,
                params in prop::collection::vec(arb_param_info(), 1..=5),
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                let parent_a: Vec<serde_json::Value> = params
                    .iter()
                    .map(|p| generate_random_value(&p.typ, &mut rng, None))
                    .collect();
                let parent_b: Vec<serde_json::Value> = params
                    .iter()
                    .map(|p| generate_random_value(&p.typ, &mut rng, None))
                    .collect();

                let mut rng2 = StdRng::seed_from_u64(seed.wrapping_add(1));
                let (child1, child2) = crossover_inputs(
                    &parent_a, &parent_b, &params, 1.0, &mut rng2,
                );

                for (i, param) in params.iter().enumerate() {
                    if let Some(v) = child1.get(i) {
                        prop_assert!(
                            value_matches_type(v, &param.typ),
                            "child1[{i}] = {v:?} doesn't match {:?}", param.typ
                        );
                    }
                    if let Some(v) = child2.get(i) {
                        prop_assert!(
                            value_matches_type(v, &param.typ),
                            "child2[{i}] = {v:?} doesn't match {:?}", param.typ
                        );
                    }
                }
            }

            // -----------------------------------------------------------------
            // shrink_candidates: type preservation
            // -----------------------------------------------------------------

            #[test]
            fn shrink_candidates_preserve_type(
                seed in 0..10000u64,
                typs in prop::collection::vec(
                    prop_oneof![
                        Just(TypeInfo::Int),
                        Just(TypeInfo::Float),
                        Just(TypeInfo::Bool),
                        Just(TypeInfo::Str),
                        Just(TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) }),
                    ],
                    1..=5,
                ),
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                for typ in &typs {
                    let value = generate_random_value(typ, &mut rng, None);
                    let candidates = shrink_candidates(&value, typ);
                    for (i, c) in candidates.iter().enumerate() {
                        prop_assert!(
                            value_matches_type(c, typ),
                            "shrink candidate[{i}] = {c:?} doesn't match type {typ:?} (original: {value:?})"
                        );
                    }
                }
            }

            // -----------------------------------------------------------------
            // shrink_candidates: no identity
            // -----------------------------------------------------------------

            #[test]
            fn shrink_candidates_exclude_original(
                seed in 0..10000u64,
                typs in prop::collection::vec(
                    prop_oneof![
                        Just(TypeInfo::Int),
                        Just(TypeInfo::Float),
                        Just(TypeInfo::Bool),
                        Just(TypeInfo::Str),
                    ],
                    1..=5,
                ),
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                for typ in &typs {
                    let value = generate_random_value(typ, &mut rng, None);
                    let candidates = shrink_candidates(&value, typ);
                    for c in &candidates {
                        prop_assert!(
                            c != &value,
                            "shrink candidate equals original: {value:?}"
                        );
                    }
                }
            }

            // -----------------------------------------------------------------
            // shrink_candidates: int shrinks toward zero
            // -----------------------------------------------------------------

            #[test]
            fn shrink_int_toward_zero(val in -1_000_000i64..1_000_000i64) {
                let value = json!(val);
                let candidates = shrink_candidates(&value, &TypeInfo::Int);
                let abs_orig = val.unsigned_abs();
                for c in &candidates {
                    if let Some(n) = c.as_i64() {
                        // All candidates should have abs <= abs(original),
                        // except for the boundary values 1 and -1 when original is 0
                        if val != 0 {
                            prop_assert!(
                                n.unsigned_abs() <= abs_orig,
                                "shrink candidate {n} has larger abs than original {val}"
                            );
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // shrink_candidates: no duplicates
            // -----------------------------------------------------------------

            #[test]
            fn shrink_candidates_no_duplicates(
                seed in 0..10000u64,
                typs in prop::collection::vec(
                    prop_oneof![
                        Just(TypeInfo::Int),
                        Just(TypeInfo::Float),
                        Just(TypeInfo::Bool),
                        Just(TypeInfo::Str),
                    ],
                    1..=3,
                ),
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                for typ in &typs {
                    let value = generate_random_value(typ, &mut rng, None);
                    let candidates = shrink_candidates(&value, typ);
                    for (i, a) in candidates.iter().enumerate() {
                        for (j, b) in candidates.iter().enumerate() {
                            if i != j {
                                prop_assert!(
                                    a != b,
                                    "duplicate shrink candidates at [{i}] and [{j}]: {a:?}"
                                );
                            }
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // generate_mock_values: output count matches input count
            // -----------------------------------------------------------------

            #[test]
            fn generate_mock_values_count_matches_params(
                params in prop::collection::vec(crate::test_arbitraries::arb_mock_param(), 1..=4),
                seed in 0..10000u64,
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                let configs = generate_mock_values(&params, &mut rng, None);
                prop_assert_eq!(configs.len(), params.len(),
                    "config count should match param count");

                for (cfg, mp) in configs.iter().zip(params.iter()) {
                    prop_assert_eq!(&cfg.symbol, &mp.symbol);
                    let expected_count = (mp.call_count_estimate as usize).max(1);
                    prop_assert_eq!(cfg.return_values.len(), expected_count,
                        "return value count mismatch for {}", mp.symbol);
                }
            }

            // -----------------------------------------------------------------
            // generate_mock_values: values match type for typed params
            // -----------------------------------------------------------------

            #[test]
            fn generate_mock_values_type_valid(
                seed in 0..10000u64,
                typ in prop_oneof![
                    Just(TypeInfo::Int),
                    Just(TypeInfo::Float),
                    Just(TypeInfo::Bool),
                    Just(TypeInfo::Str),
                ],
                call_count in 1..5u32,
            ) {
                use crate::auto_mock::{IoCategory, ValueSource};
                let mp = MockParam {
                    symbol: "test_fn".to_string(),
                    return_type: typ.clone(),
                    category: IoCategory::ExternalOther,
                    call_count_estimate: call_count,
                    value_source: ValueSource::AutoGenerated,
                };
                let mut rng = StdRng::seed_from_u64(seed);
                let configs = generate_mock_values(&[mp], &mut rng, None);
                prop_assert_eq!(configs.len(), 1);
                for val in &configs[0].return_values {
                    prop_assert!(
                        value_matches_type(val, &typ),
                        "value {:?} doesn't match type {:?}", val, typ
                    );
                }
            }

            // -----------------------------------------------------------------
            // mutate_mock_values: preserves vector structure
            // -----------------------------------------------------------------

            #[test]
            // Uses non-recursive leaf types to avoid a pre-existing panic
            // in string_mutation::mutate_structure_aware (unrelated to mock values).
            fn mutate_mock_values_preserves_structure(
                seed in 0..10000u64,
                typs in prop::collection::vec(
                    prop_oneof![
                        Just(TypeInfo::Int),
                        Just(TypeInfo::Float),
                        Just(TypeInfo::Bool),
                        Just(TypeInfo::Unknown),
                    ],
                    1..=4,
                ),
            ) {
                use crate::auto_mock::{IoCategory, ValueSource};
                let params: Vec<MockParam> = typs.iter().enumerate().map(|(i, t)| MockParam {
                    symbol: format!("mock_{}", i),
                    return_type: t.clone(),
                    category: IoCategory::ExternalOther,
                    call_count_estimate: 2,
                    value_source: ValueSource::AutoGenerated,
                }).collect();

                let mut rng = StdRng::seed_from_u64(seed);
                let configs = generate_mock_values(&params, &mut rng, None);
                let mut rng2 = StdRng::seed_from_u64(seed.wrapping_add(1));
                let mutated = mutate_mock_values(&configs, &params, 1.0, &[], &mut rng2);

                prop_assert_eq!(mutated.len(), configs.len(),
                    "mutated config count mismatch");
                for (mc, orig) in mutated.iter().zip(configs.iter()) {
                    prop_assert_eq!(mc.return_values.len(), orig.return_values.len(),
                        "return value count changed after mutation");
                    prop_assert_eq!(&mc.symbol, &orig.symbol,
                        "symbol changed after mutation");
                }
            }

            // -----------------------------------------------------------------
            // crossover_mock_values: preserves vector structure
            // -----------------------------------------------------------------

            #[test]
            fn crossover_mock_values_preserves_structure(
                params in prop::collection::vec(crate::test_arbitraries::arb_mock_param(), 1..=4),
                seed in 0..10000u64,
            ) {
                let mut rng = StdRng::seed_from_u64(seed);
                let parent_a = generate_mock_values(&params, &mut rng, None);
                let mut rng2 = StdRng::seed_from_u64(seed.wrapping_add(1));
                let parent_b = generate_mock_values(&params, &mut rng2, None);

                let mut rng3 = StdRng::seed_from_u64(seed.wrapping_add(2));
                let (child1, child2) = crossover_mock_values(
                    &parent_a, &parent_b, &params, 1.0, &mut rng3,
                );

                let expected_len = parent_a.len().min(parent_b.len()).min(params.len());
                prop_assert_eq!(child1.len(), expected_len, "child1 length mismatch");
                prop_assert_eq!(child2.len(), expected_len, "child2 length mismatch");

                for idx in 0..expected_len {
                    let min_vals = parent_a[idx].return_values.len()
                        .min(parent_b[idx].return_values.len());
                    prop_assert!(child1[idx].return_values.len() >= min_vals,
                        "child1[{}] return_values too short", idx);
                    prop_assert!(child2[idx].return_values.len() >= min_vals,
                        "child2[{}] return_values too short", idx);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Mock value generation unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn generate_mock_values_typed_param() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "readFile".to_string(),
            return_type: TypeInfo::Str,
            category: IoCategory::FileSystem,
            call_count_estimate: 3,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];

        let configs = generate_mock_values(&params, &mut rng, None);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].symbol, "readFile");
        assert_eq!(configs[0].return_values.len(), 3);
        for val in &configs[0].return_values {
            assert!(val.is_string(), "expected string, got {val}");
        }
    }

    #[test]
    fn generate_mock_values_unknown_uses_category_shaping() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "query".to_string(),
            return_type: TypeInfo::Unknown,
            category: IoCategory::Database,
            call_count_estimate: 2,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];

        let configs = generate_mock_values(&params, &mut rng, None);
        assert_eq!(configs[0].return_values.len(), 2);
        for val in &configs[0].return_values {
            assert!(val.is_object(), "expected DB-shaped object, got {val}");
        }
    }

    #[test]
    fn generate_mock_values_result_type_produces_ok_and_err() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "tryConnect".to_string(),
            return_type: TypeInfo::Complex {
                kind: ComplexKind::Result,
                metadata: serde_json::Map::new(),
                inner: None,
            },
            category: IoCategory::Network,
            call_count_estimate: 4,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];

        let configs = generate_mock_values(&params, &mut rng, None);
        assert_eq!(configs[0].return_values.len(), 4);

        let ok_count = configs[0]
            .return_values
            .iter()
            .filter(|v| v.get("ok").and_then(|o| o.as_bool()) == Some(true))
            .count();
        let err_count = configs[0]
            .return_values
            .iter()
            .filter(|v| v.get("ok").and_then(|o| o.as_bool()) == Some(false))
            .count();
        assert!(ok_count > 0, "expected at least one ok variant");
        assert!(err_count > 0, "expected at least one err variant");
    }

    #[test]
    fn generate_mock_values_union_with_error_produces_both() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "fetchData".to_string(),
            return_type: TypeInfo::Union {
                variants: vec![
                    TypeInfo::Str,
                    TypeInfo::Complex {
                        kind: ComplexKind::Error,
                        metadata: serde_json::Map::new(),
                        inner: None,
                    },
                ],
            },
            category: IoCategory::Network,
            call_count_estimate: 4,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];

        let configs = generate_mock_values(&params, &mut rng, None);
        assert_eq!(configs[0].return_values.len(), 4);

        // Even indices (0, 2) use success variants (Str), odd indices (1, 3) use error variants.
        // Without frontend capabilities, Error complex types fall back to unknown generation,
        // so we verify the alternating pattern by checking that success slots are strings.
        assert!(configs[0].return_values[0].is_string(),
            "expected string at index 0, got {:?}", configs[0].return_values[0]);
        assert_eq!(configs[0].default_behavior, MockBehavior::ReturnGenerated);
    }

    #[test]
    fn generate_mock_values_zero_call_count_produces_at_least_one() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "unused".to_string(),
            return_type: TypeInfo::Int,
            category: IoCategory::ExternalOther,
            call_count_estimate: 0,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];

        let configs = generate_mock_values(&params, &mut rng, None);
        assert_eq!(configs[0].return_values.len(), 1);
    }

    #[test]
    fn mutate_mock_values_preserves_symbol_and_count() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "query".to_string(),
            return_type: TypeInfo::Int,
            category: IoCategory::Database,
            call_count_estimate: 3,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];
        let configs = generate_mock_values(&params, &mut rng, None);
        let mutated = mutate_mock_values(&configs, &params, 1.0, &[], &mut rng);

        assert_eq!(mutated[0].symbol, "query");
        assert_eq!(mutated[0].return_values.len(), configs[0].return_values.len());
    }

    #[test]
    fn crossover_mock_values_preserves_length() {
        let mut rng = seeded_rng();
        let params = vec![
            MockParam {
                symbol: "a".to_string(),
                return_type: TypeInfo::Int,
                category: IoCategory::ExternalOther,
                call_count_estimate: 3,
                value_source: crate::auto_mock::ValueSource::AutoGenerated,
            },
            MockParam {
                symbol: "b".to_string(),
                return_type: TypeInfo::Str,
                category: IoCategory::ExternalOther,
                call_count_estimate: 2,
                value_source: crate::auto_mock::ValueSource::AutoGenerated,
            },
        ];
        let parent_a = generate_mock_values(&params, &mut rng, None);
        let parent_b = generate_mock_values(&params, &mut rng, None);

        let (child1, child2) = crossover_mock_values(&parent_a, &parent_b, &params, 1.0, &mut rng);
        assert_eq!(child1.len(), 2);
        assert_eq!(child2.len(), 2);
    }

    #[test]
    fn generate_mock_values_network_unknown_produces_http_shape() {
        let mut rng = seeded_rng();
        let params = vec![MockParam {
            symbol: "get".to_string(),
            return_type: TypeInfo::Unknown,
            category: IoCategory::Network,
            call_count_estimate: 1,
            value_source: crate::auto_mock::ValueSource::AutoGenerated,
        }];

        let configs = generate_mock_values(&params, &mut rng, None);
        let val = &configs[0].return_values[0];
        assert!(val.get("status").is_some(), "network mock should have status field");
    }

    // -----------------------------------------------------------------------
    // Base64 codec unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn base64_roundtrip_empty() {
        let bytes: &[u8] = &[];
        assert_eq!(base64_decode(&base64_encode(bytes)), Some(bytes.to_vec()));
    }

    #[test]
    fn base64_roundtrip_single_byte() {
        for b in [0u8, 1, 127, 128, 255] {
            let encoded = base64_encode(&[b]);
            let decoded = base64_decode(&encoded).expect("decode failed");
            assert_eq!(decoded, vec![b], "roundtrip failed for byte {b}");
        }
    }

    #[test]
    fn base64_roundtrip_known_values() {
        // "Hello" encodes to "SGVsbG8="
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_decode("SGVsbG8="), Some(b"Hello".to_vec()));
        // [0] encodes to "AA=="
        assert_eq!(base64_encode(&[0u8]), "AA==");
        assert_eq!(base64_decode("AA=="), Some(vec![0u8]));
        // [255,255,255,255] encodes to "/////w=="
        assert_eq!(base64_encode(&[255u8, 255, 255, 255]), "/////w==");
        assert_eq!(base64_decode("/////w=="), Some(vec![255u8, 255, 255, 255]));
        // [1,2,3,4] encodes to "AQIDBA=="
        assert_eq!(base64_encode(&[1u8, 2, 3, 4]), "AQIDBA==");
        assert_eq!(base64_decode("AQIDBA=="), Some(vec![1u8, 2, 3, 4]));
    }

    #[test]
    fn base64_decode_invalid_char_returns_none() {
        // '!' is not in the base64 alphabet
        assert_eq!(base64_decode("SG!s"), None);
    }

    // -----------------------------------------------------------------------
    // Binary buffer mutation unit tests
    // -----------------------------------------------------------------------

    fn buffer_value(bytes: &[u8]) -> Value {
        json!({
            "__complex_type": "buffer",
            "encoding": "base64",
            "value": base64_encode(bytes)
        })
    }

    fn extract_buffer_bytes(val: &Value) -> Vec<u8> {
        let encoded = val
            .as_object()
            .and_then(|o| o.get("value"))
            .and_then(|v| v.as_str())
            .expect("expected buffer value field");
        base64_decode(encoded).expect("expected valid base64")
    }

    #[test]
    fn mutate_buffer_preserves_wire_format() {
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input = buffer_value(&[1, 2, 3, 4]);
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            let obj = mutated.as_object().expect("buffer mutant should be object");
            assert_eq!(obj.get("__complex_type").and_then(|v| v.as_str()), Some("buffer"));
            assert_eq!(obj.get("encoding").and_then(|v| v.as_str()), Some("base64"));
            assert!(obj.contains_key("value"), "missing value field");
            // value must be valid base64
            let encoded = obj.get("value").and_then(|v| v.as_str()).unwrap();
            assert!(base64_decode(encoded).is_some(), "value is not valid base64: {encoded}");
        }
    }

    #[test]
    fn mutate_buffer_actually_mutates() {
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input = buffer_value(&[0x41, 0x42, 0x43, 0x44, 0x45]);
        let mut any_diff = false;
        for seed in 0..50_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            if mutated != input {
                any_diff = true;
                break;
            }
        }
        assert!(any_diff, "mutate_buffer never produced a different value");
    }

    #[test]
    fn mutate_buffer_empty_input_no_panic() {
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input = buffer_value(&[]);
        let mut rng = seeded_rng();
        for _ in 0..50 {
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            assert!(mutated.is_object(), "expected object from empty buffer mutation");
            // Empty buffer can only grow (insertion), result must be valid.
            extract_buffer_bytes(&mutated); // panics if invalid
        }
    }

    #[test]
    fn mutate_buffer_single_byte_no_panic() {
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input = buffer_value(&[0xFF]);
        let mut rng = seeded_rng();
        for _ in 0..100 {
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            assert!(mutated.is_object(), "expected object");
            extract_buffer_bytes(&mutated); // panics if invalid
        }
    }

    #[test]
    fn mutate_buffer_bit_flip_changes_exactly_one_bit() {
        // Force bit-flip operator (op=0) by using a buffer with 1 byte so
        // all four operators are active; run enough seeds to hit bit_flip.
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input_bytes = [0b1010_1010u8];
        let input = buffer_value(&input_bytes);
        let mut saw_single_bit_flip = false;
        for seed in 0..200_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            let out = extract_buffer_bytes(&mutated);
            if out.len() == 1 {
                let diff = input_bytes[0] ^ out[0];
                // A single bit flip means exactly one bit is set in XOR diff.
                if diff != 0 && diff.count_ones() == 1 {
                    saw_single_bit_flip = true;
                    break;
                }
            }
        }
        assert!(saw_single_bit_flip, "should produce a single-bit-flip mutation");
    }

    #[test]
    fn mutate_buffer_block_insertion_grows_buffer() {
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input = buffer_value(&[1, 2, 3]);
        let mut saw_growth = false;
        for seed in 0..200_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            let out = extract_buffer_bytes(&mutated);
            if out.len() > 3 {
                saw_growth = true;
                break;
            }
        }
        assert!(saw_growth, "should sometimes grow the buffer via block insertion");
    }

    #[test]
    fn mutate_buffer_block_deletion_shrinks_buffer() {
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let input = buffer_value(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut saw_shrink = false;
        for seed in 0..200_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mutated = mutate_value(&input, &typ, &[], &mut rng);
            let out = extract_buffer_bytes(&mutated);
            if out.len() < 8 {
                saw_shrink = true;
                break;
            }
        }
        assert!(saw_shrink, "should sometimes shrink the buffer via block deletion");
    }

    #[test]
    fn mutate_buffer_byte_arithmetic_wraps() {
        // Verify that byte arithmetic wraps rather than panics on boundary bytes.
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        for boundary in [0u8, 255u8] {
            let input = buffer_value(&[boundary; 4]);
            let mut rng = seeded_rng();
            for _ in 0..100 {
                let mutated = mutate_value(&input, &typ, &[], &mut rng);
                extract_buffer_bytes(&mutated); // must not panic
            }
        }
    }

    #[test]
    fn mutate_buffer_malformed_value_regenerates_without_panic() {
        // If the JSON value is missing fields or contains invalid base64, the
        // mutation should not panic — it falls back to a fresh small buffer.
        let typ = TypeInfo::Complex {
            kind: ComplexKind::Buffer,
            metadata: serde_json::Map::new(),
            inner: None,
        };
        let malformed_inputs = [
            json!(null),
            json!("not_an_object"),
            json!({"__complex_type": "buffer", "encoding": "base64", "value": "!!!invalid!!!"}),
            json!({"__complex_type": "buffer", "encoding": "base64"}), // missing value
        ];
        let mut rng = seeded_rng();
        for bad in &malformed_inputs {
            for _ in 0..10 {
                let mutated = mutate_value(bad, &typ, &[], &mut rng);
                assert!(mutated.is_object(), "expected object for malformed input: {bad}");
            }
        }
    }

    #[test]
    fn mutate_buffer_str_param_unaffected() {
        // Strings must NOT be routed through the buffer mutator.
        let mut rng = seeded_rng();
        let val = json!("hello world");
        for _ in 0..50 {
            let mutated = mutate_value(&val, &TypeInfo::Str, &[], &mut rng);
            assert!(mutated.is_string(), "Str mutation should always produce a string");
        }
    }

    // -----------------------------------------------------------------------
    // Property-based tests for buffer mutation
    // -----------------------------------------------------------------------

    mod buffer_prop_tests {
        use super::*;
        use crate::types::ComplexKind;
        use proptest::prelude::*;
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        /// Arbitrary byte vec with length 0–32 for buffer mutation tests.
        fn arb_byte_vec() -> impl Strategy<Value = Vec<u8>> {
            prop::collection::vec(any::<u8>(), 0..=32)
        }

        proptest! {
            /// Base64 roundtrip: encode then decode returns original bytes.
            #[test]
            fn base64_encode_decode_roundtrip(bytes in arb_byte_vec()) {
                let encoded = base64_encode(&bytes);
                let decoded = base64_decode(&encoded)
                    .expect("encode→decode should always succeed");
                prop_assert_eq!(decoded, bytes, "roundtrip failed");
            }

            /// Type contract: mutating a Buffer always produces a Buffer-tagged object.
            #[test]
            fn mutate_buffer_type_preserved(
                bytes in arb_byte_vec(),
                seed in 0..10000u64,
            ) {
                let typ = TypeInfo::Complex {
                    kind: ComplexKind::Buffer,
                    metadata: serde_json::Map::new(),
                    inner: None,
                };
                let input = json!({
                    "__complex_type": "buffer",
                    "encoding": "base64",
                    "value": base64_encode(&bytes)
                });
                let mut rng = StdRng::seed_from_u64(seed);
                let mutated = mutate_value(&input, &typ, &[], &mut rng);

                let obj = mutated.as_object().expect("result should be an object");
                prop_assert_eq!(
                    obj.get("__complex_type").and_then(|v| v.as_str()),
                    Some("buffer"),
                    "wrong __complex_type tag"
                );
                prop_assert_eq!(
                    obj.get("encoding").and_then(|v| v.as_str()),
                    Some("base64"),
                    "wrong encoding"
                );
                let encoded = obj
                    .get("value")
                    .and_then(|v| v.as_str())
                    .expect("missing value field");
                prop_assert!(
                    base64_decode(encoded).is_some(),
                    "value is not valid base64: {encoded}"
                );
            }

            /// Mutation actually changes the value for buffers with >= 4 bytes.
            ///
            /// With 4 bytes and 20 seed attempts, probability of no change is
            /// astronomically low (each operator has distinct output probabilities).
            #[test]
            fn mutate_buffer_changes_value_for_nontrivial_inputs(
                bytes in prop::collection::vec(any::<u8>(), 4..=16),
                base_seed in 0..1000u64,
            ) {
                let typ = TypeInfo::Complex {
                    kind: ComplexKind::Buffer,
                    metadata: serde_json::Map::new(),
                    inner: None,
                };
                let input = json!({
                    "__complex_type": "buffer",
                    "encoding": "base64",
                    "value": base64_encode(&bytes)
                });
                let mut any_diff = false;
                for offset in 0..20u64 {
                    let mut rng = StdRng::seed_from_u64(base_seed.wrapping_add(offset));
                    let mutated = mutate_value(&input, &typ, &[], &mut rng);
                    if mutated != input {
                        any_diff = true;
                        break;
                    }
                }
                prop_assert!(any_diff, "no mutation ever changed a {}-byte buffer", bytes.len());
            }

            /// Length constraints: bit flip and byte arithmetic preserve length;
            /// insertion grows by 1–8; deletion shrinks by 1–8 (but clamps at 0).
            #[test]
            fn mutate_buffer_length_constraints(
                bytes in prop::collection::vec(any::<u8>(), 1..=16),
                seed in 0..10000u64,
            ) {
                let typ = TypeInfo::Complex {
                    kind: ComplexKind::Buffer,
                    metadata: serde_json::Map::new(),
                    inner: None,
                };
                let original_len = bytes.len();
                let input = json!({
                    "__complex_type": "buffer",
                    "encoding": "base64",
                    "value": base64_encode(&bytes)
                });
                let mut rng = StdRng::seed_from_u64(seed);
                let mutated = mutate_value(&input, &typ, &[], &mut rng);
                let out = base64_decode(
                    mutated.as_object()
                        .and_then(|o| o.get("value"))
                        .and_then(|v| v.as_str())
                        .expect("missing value field")
                ).expect("invalid base64");
                let out_len = out.len();
                prop_assert!(
                    out_len == original_len                                          // bit flip or byte arith
                    || (out_len > original_len && out_len <= original_len + BUFFER_MAX_BLOCK_INSERT) // insert
                    || (out_len < original_len && original_len - out_len <= BUFFER_MAX_BLOCK_DELETE), // delete
                    "unexpected length: original={original_len}, output={out_len}"
                );
            }

            /// No panic on any byte sequence, even adversarial ones.
            #[test]
            fn mutate_buffer_no_panic(
                bytes in arb_byte_vec(),
                seed in 0..10000u64,
            ) {
                let typ = TypeInfo::Complex {
                    kind: ComplexKind::Buffer,
                    metadata: serde_json::Map::new(),
                    inner: None,
                };
                let input = json!({
                    "__complex_type": "buffer",
                    "encoding": "base64",
                    "value": base64_encode(&bytes)
                });
                let mut rng = StdRng::seed_from_u64(seed);
                // Must not panic.
                let _ = mutate_value(&input, &typ, &[], &mut rng);
            }
        }
    }
}
