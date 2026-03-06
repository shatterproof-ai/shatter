// Example 14: Semantic versioning parser and range matcher.
// Numeric parsing, string splitting, multi-field comparison, and operator dispatch.

#[derive(Debug)]
struct Semver {
    major: i32,
    minor: i32,
    patch: i32,
    prerelease: String,
}

/// parse_semver — 12 branches: empty→error, <3 parts→error,
/// non-numeric major/minor/patch→error, negative→error,
/// valid x.y.z→Ok, with prerelease→splits on first hyphen,
/// 'v' prefix stripped, extra parts ignored.
fn parse_semver(version: &str) -> Result<Semver, String> {
    if version.is_empty() {
        return Err("invalid semver".to_string());
    }

    let v = if version.starts_with('v') || version.starts_with('V') {
        &version[1..]
    } else {
        version
    };

    let parts: Vec<&str> = v.splitn(3, '.').collect();
    if parts.len() < 3 {
        return Err("invalid semver".to_string());
    }

    let major: i32 = parts[0].parse().map_err(|_| "invalid semver".to_string())?;
    let minor: i32 = parts[1].parse().map_err(|_| "invalid semver".to_string())?;

    let (patch_str, prerelease) = match parts[2].find('-') {
        Some(idx) => (&parts[2][..idx], parts[2][idx + 1..].to_string()),
        None => (parts[2], String::new()),
    };

    let patch: i32 = patch_str.parse().map_err(|_| "invalid semver".to_string())?;

    if major < 0 || minor < 0 || patch < 0 {
        return Err("invalid semver".to_string());
    }

    Ok(Semver { major, minor, patch, prerelease })
}

fn compare_semver(a: &Semver, b: &Semver) -> i32 {
    if a.major != b.major { return a.major - b.major; }
    if a.minor != b.minor { return a.minor - b.minor; }
    if a.patch != b.patch { return a.patch - b.patch; }

    if a.prerelease.is_empty() && b.prerelease.is_empty() { return 0; }
    if a.prerelease.is_empty() { return 1; }
    if b.prerelease.is_empty() { return -1; }
    a.prerelease.cmp(&b.prerelease) as i32
}

/// satisfies_range — 19 branches: exact match, >=, >, <=, <,
/// ^ (caret) same major + cmp, ^ different major, ~ (tilde) same major.minor + cmp,
/// ~ different minor, wildcard *, x→true, unknown operator→error.
fn satisfies_range(version: &str, range: &str) -> Result<bool, String> {
    if range == "*" || range == "x" {
        return Ok(true);
    }

    let (operator, range_version) = if range.starts_with(">=") {
        (">=", &range[2..])
    } else if range.starts_with('>') {
        (">", &range[1..])
    } else if range.starts_with("<=") {
        ("<=", &range[2..])
    } else if range.starts_with('<') {
        ("<", &range[1..])
    } else if range.starts_with('^') {
        ("^", &range[1..])
    } else if range.starts_with('~') {
        ("~", &range[1..])
    } else {
        ("", range)
    };

    let ver = parse_semver(version)?;
    let target = parse_semver(range_version)?;
    let cmp = compare_semver(&ver, &target);

    match operator {
        "" => Ok(cmp == 0),
        ">=" => Ok(cmp >= 0),
        ">" => Ok(cmp > 0),
        "<=" => Ok(cmp <= 0),
        "<" => Ok(cmp < 0),
        "^" => {
            if ver.major != target.major {
                Ok(false)
            } else {
                Ok(cmp >= 0)
            }
        }
        "~" => {
            if ver.major != target.major || ver.minor != target.minor {
                Ok(false)
            } else {
                Ok(cmp >= 0)
            }
        }
        _ => Err("unknown range operator".to_string()),
    }
}

fn main() {
    println!("{:?}", parse_semver("1.2.3-alpha"));
    println!("{:?}", satisfies_range("1.5.0", "^1.2.0"));
    println!("{:?}", satisfies_range("2.0.0", "~1.2.0"));
}
