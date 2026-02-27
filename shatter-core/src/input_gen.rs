//! Random input generation from TypeInfo metadata.
//!
//! Generates random JSON values matching the type signatures reported by
//! language frontends. Used for the initial exploration phase before symbolic
//! constraint solving kicks in.

use rand::Rng;
use serde_json::{json, Value};

use crate::orchestrator::FrontendCapabilities;
use crate::types::{ComplexKind, TypeInfo};

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
        "https://例え.jp/テスト",
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
        5 => ("€", 8364),         // currency symbol
        6 => ("🎉", 127881),     // emoji
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
        .map(|p| generate_random_value(&p.typ, rng, caps))
        .collect()
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
            ParamInfo { name: "a".into(), typ: TypeInfo::Int },
            ParamInfo { name: "b".into(), typ: TypeInfo::Str },
            ParamInfo { name: "c".into(), typ: TypeInfo::Bool },
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
        // Empty (0) and single-element (1) should each be ~25% of results.
        // With 1000 trials, expect at least 150 each (well below 25%).
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
        // Small sizes (0-3) should dominate: at least 75% of results.
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
        // No caps → falls back to Unknown generation
        let val = generate_random_value(&typ, &mut rng, None);
        // Should be a primitive, not a tagged object
        assert!(
            val.as_object().and_then(|o| o.get("__complex_type")).is_none(),
            "without caps, should not produce tagged complex: {val}"
        );
    }
}
