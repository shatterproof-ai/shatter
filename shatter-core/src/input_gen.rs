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
        .map(|p| generate_random_value(&p.typ, rng, caps))
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
                    eprintln!(
                        "[shatter-core] generator error for {name} ({file}): {message}"
                    );
                }
                _ => {
                    // Unexpected response type -- skip this generator.
                    eprintln!(
                        "[shatter-core] unexpected response for generator {name}"
                    );
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
        .map(|(param, source)| match source {
            ValueSource::CustomGenerator {
                generator_name,
                generator_file,
                ..
            } => {
                let file_str = generator_file.display().to_string();
                prefetched
                    .take(&file_str, generator_name)
                    .unwrap_or_else(|| generate_random_value(&param.typ, rng, caps))
            }
            ValueSource::BuiltIn => generate_random_value(&param.typ, rng, caps),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Type-aware mutation operators
// ---------------------------------------------------------------------------

/// Mutate a single JSON value according to its type.
///
/// Applies a random type-appropriate mutation operator. The output is always
/// type-valid for the given `TypeInfo`. Types that cannot be meaningfully
/// mutated (Complex, Opaque, Unknown) are returned unchanged.
pub fn mutate_value(value: &Value, typ: &TypeInfo, rng: &mut impl Rng) -> Value {
    match typ {
        TypeInfo::Int => mutate_int(value, rng),
        TypeInfo::Float => mutate_float(value, rng),
        TypeInfo::Bool => mutate_bool(value),
        TypeInfo::Str => mutate_string(value, rng),
        TypeInfo::Array { element } => mutate_array(value, element, rng),
        TypeInfo::Object { fields } => mutate_object(value, fields, rng),
        TypeInfo::Union { variants } => mutate_union(value, variants, rng),
        TypeInfo::Nullable { inner } => mutate_nullable(value, inner, rng),
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
    rng: &mut impl Rng,
) -> Vec<Value> {
    inputs
        .iter()
        .zip(params.iter())
        .map(|(val, param)| {
            if rng.random_range(0.0..1.0_f64) < mutation_rate {
                mutate_value(val, &param.typ, rng)
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
fn mutate_string(value: &Value, rng: &mut impl Rng) -> Value {
    let s = match value.as_str() {
        Some(s) => s.to_string(),
        None => return generate_string(rng),
    };
    let op: u8 = rng.random_range(0..6);
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
        _ => {
            // Long string (1000 chars)
            let long: String = (0..1000).map(|_| 'a').collect();
            json!(long)
        }
    }
}

/// Mutate an array value.
fn mutate_array(value: &Value, element: &TypeInfo, rng: &mut impl Rng) -> Value {
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
            result[idx] = mutate_value(&result[idx], element, rng);
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
                let mutated = mutate_value(current, typ, rng);
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
fn mutate_union(value: &Value, variants: &[TypeInfo], rng: &mut impl Rng) -> Value {
    if variants.is_empty() {
        return value.clone();
    }
    let idx = rng.random_range(0..variants.len());
    mutate_value(value, &variants[idx], rng)
}

/// Mutate a nullable value: 20% chance to flip null/non-null, otherwise mutate inner.
fn mutate_nullable(value: &Value, inner: &TypeInfo, rng: &mut impl Rng) -> Value {
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
        mutate_value(value, inner, rng)
    }
}

// ---------------------------------------------------------------------------
// Type-aware crossover operators
// ---------------------------------------------------------------------------

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
            let mutated = mutate_value(&json!(42), &TypeInfo::Int, &mut rng);
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
        let mutated = mutate_value(&json!("not_an_int"), &TypeInfo::Int, &mut rng);
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
            let mutated = mutate_value(&json!(1.0), &TypeInfo::Float, &mut rng);
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
        assert_eq!(mutate_value(&json!(true), &TypeInfo::Bool, &mut rng), json!(false));
        assert_eq!(mutate_value(&json!(false), &TypeInfo::Bool, &mut rng), json!(true));
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
            let mutated = mutate_value(&json!(original), &TypeInfo::Str, &mut rng);
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
            let mutated = mutate_value(&json!(""), &TypeInfo::Str, &mut rng);
            assert!(mutated.is_string(), "expected string, got {mutated}");
        }
    }

    #[test]
    fn mutate_array_type_valid() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };
        for _ in 0..100 {
            let mutated = mutate_value(&json!([1, 2, 3]), &typ, &mut rng);
            assert!(mutated.is_array(), "expected array, got {mutated}");
        }
    }

    #[test]
    fn mutate_array_empty_can_grow() {
        let mut rng = seeded_rng();
        let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };
        let mutated = mutate_value(&json!([]), &typ, &mut rng);
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
            let mutated = mutate_value(&original, &typ, &mut rng);
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
        let mutated = mutate_inputs(&inputs, &params, 0.0, &mut rng);
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
        let mutated = mutate_inputs(&inputs, &params, 1.0, &mut rng);
        // Bools always flip, so both should change
        assert_eq!(mutated[0], json!(false));
        assert_eq!(mutated[1], json!(true));
    }

    #[test]
    fn mutate_value_unknown_returns_unchanged() {
        let mut rng = seeded_rng();
        let val = json!(42);
        assert_eq!(mutate_value(&val, &TypeInfo::Unknown, &mut rng), val);
    }

    #[test]
    fn mutate_value_opaque_returns_unchanged() {
        let mut rng = seeded_rng();
        let val = json!(null);
        let typ = TypeInfo::Opaque { label: "net.Socket".into() };
        assert_eq!(mutate_value(&val, &typ, &mut rng), val);
    }

    #[test]
    fn mutate_nullable_can_flip_to_null() {
        let mut rng = StdRng::seed_from_u64(0);
        let typ = TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) };
        let mut saw_null = false;
        let mut saw_value = false;
        for _ in 0..100 {
            let mutated = mutate_value(&json!(42), &typ, &mut rng);
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
            let mutated = mutate_value(&Value::Null, &typ, &mut rng);
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
            let mutated = mutate_value(&json!(42), &typ, &mut rng);
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
}
