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

/// Pre-fetched values from custom generators, keyed by `(generator_file, generator_name)`.
///
/// Each entry holds a queue of values that can be drawn from during input generation.
/// When the queue is empty, generation falls back to built-in.
#[derive(Debug, Default)]
pub struct PrefetchedValues {
    /// Map from (generator file path as string, generator name) to queued values.
    values: std::collections::HashMap<(String, String), Vec<Value>>,
}

impl PrefetchedValues {
    /// Create an empty prefetch store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: std::collections::HashMap::new(),
        }
    }

    /// Insert generated values for a specific generator.
    pub fn insert(&mut self, file: String, name: String, vals: Vec<Value>) {
        self.values.entry((file, name)).or_default().extend(vals);
    }

    /// Take the next value for a generator, if available.
    pub fn take(&mut self, file: &str, name: &str) -> Option<Value> {
        let key = (file.to_string(), name.to_string());
        let queue = self.values.get_mut(&key)?;
        if queue.is_empty() {
            None
        } else {
            Some(queue.remove(0))
        }
    }

    /// Check whether a generator has remaining values.
    #[must_use]
    pub fn has_values(&self, file: &str, name: &str) -> bool {
        self.values
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
                })
                .await?;

            match response.result {
                crate::protocol::ResponseResult::Generate { value } => {
                    store.insert(file.clone(), name.clone(), vec![value]);
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
}
