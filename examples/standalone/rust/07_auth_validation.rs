// Example 7: Auth and token validation.
// Tests reasoning about string structure, base64 decoding, and multi-step validation.

use std::collections::HashMap;

/// validate_jwt — 10 branches: empty token→"invalid: empty token",
/// not 3 parts→"invalid: malformed token", bad header base64→"invalid: corrupt header",
/// alg!="HS256"→"invalid: unsupported algorithm", bad payload base64→"invalid: corrupt payload",
/// expired→"invalid: token expired", wrong issuer→"invalid: wrong issuer",
/// wrong audience→"invalid: wrong audience", missing scope→"invalid: insufficient scope",
/// all pass→"valid".
///
/// Simplified: uses naive key=value pairs instead of real JSON/base64 to keep stdlib-only.
fn validate_jwt(
    token: &str,
    expected_issuer: &str,
    expected_audience: &str,
    required_scope: &str,
    now_epoch_seconds: i64,
) -> &'static str {
    if token.is_empty() {
        return "invalid: empty token";
    }

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return "invalid: malformed token";
    }

    let header = match parse_simple_kv(parts[0]) {
        Some(m) => m,
        None => return "invalid: corrupt header",
    };

    if header.get("alg").map(|s| s.as_str()) != Some("HS256") {
        return "invalid: unsupported algorithm";
    }

    let payload = match parse_simple_kv(parts[1]) {
        Some(m) => m,
        None => return "invalid: corrupt payload",
    };

    if let Some(exp_str) = payload.get("exp") {
        if let Ok(exp) = exp_str.parse::<i64>() {
            if exp < now_epoch_seconds {
                return "invalid: token expired";
            }
        }
    }

    if payload.get("iss").map(|s| s.as_str()) != Some(expected_issuer) {
        return "invalid: wrong issuer";
    }

    let aud_match = payload.get("aud").map_or(false, |aud| {
        aud.split(',').any(|a| a.trim() == expected_audience)
    });
    if !aud_match {
        return "invalid: wrong audience";
    }

    let scope_match = payload.get("scope").map_or(false, |scope| {
        scope.split(' ').any(|s| s == required_scope)
    });
    if !scope_match {
        return "invalid: insufficient scope";
    }

    "valid"
}

/// Parses "key1=val1;key2=val2" into a HashMap. Returns None on malformed input.
fn parse_simple_kv(s: &str) -> Option<HashMap<String, String>> {
    if s.is_empty() {
        return None;
    }
    let mut map = HashMap::new();
    for pair in s.split(';') {
        let parts: Vec<&str> = pair.splitn(2, '=').collect();
        if parts.len() != 2 {
            return None;
        }
        map.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
    }
    Some(map)
}

struct User {
    id: String,
    roles: Vec<String>,
    active: bool,
}

const VALID_ACTIONS: &[&str] = &["read", "write", "delete"];

/// authorize_request — 12 branches: no roles→denied, inactive→denied,
/// empty resource→denied, invalid action→denied, superadmin→granted,
/// owner+read→granted, owner+write→granted, owner+delete→granted,
/// admin+read→granted, admin+write→granted, admin+delete→denied,
/// viewer+read→granted, viewer+other→denied, no matching role→denied.
fn authorize_request(
    user: &User,
    resource: &str,
    resource_owner_id: &str,
    action: &str,
) -> &'static str {
    if user.roles.is_empty() {
        return "denied: no roles";
    }
    if !user.active {
        return "denied: inactive user";
    }
    if resource.is_empty() {
        return "denied: invalid resource";
    }
    if !VALID_ACTIONS.contains(&action) {
        return "denied: invalid action";
    }

    if user.roles.iter().any(|r| r == "superadmin") {
        return "granted: superadmin";
    }

    let is_owner = user.id == resource_owner_id;

    if is_owner {
        return match action {
            "read" => "granted: owner-read",
            "write" => "granted: owner-write",
            "delete" => "granted: owner-delete",
            _ => "denied: invalid action",
        };
    }

    if user.roles.iter().any(|r| r == "admin") {
        return match action {
            "read" => "granted: admin-read",
            "write" => "granted: admin-write",
            _ => "denied: admin-no-delete",
        };
    }

    if user.roles.iter().any(|r| r == "viewer") {
        if action == "read" {
            return "granted: viewer";
        }
        return "denied: viewer-readonly";
    }

    "denied: no matching role"
}

fn main() {
    let token = "alg=HS256.iss=myapp;aud=web;scope=admin read;exp=9999999999.signature";
    println!("{}", validate_jwt(token, "myapp", "web", "read", 1000));

    let user = User {
        id: "u1".to_string(),
        roles: vec!["admin".to_string()],
        active: true,
    };
    println!("{}", authorize_request(&user, "/docs", "u2", "read"));
}
