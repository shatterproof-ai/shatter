// Example 15: Email address validator.
// Character-class checks, length limits, and structural validation per RFC 5321/5322 subset.

const LOCAL_PART_MAX: usize = 64;
const DOMAIN_MAX: usize = 253;
const DOMAIN_LABEL_MAX: usize = 63;

struct EmailResult {
    valid: bool,
    reason: Option<String>,
    tag: Option<String>,
    quoted: bool,
}

fn is_valid_local_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '.' | '!' | '#' | '$' | '%' | '&' | '\'' | '*' | '+' | '/'
            | '=' | '?' | '^' | '_' | '`' | '{' | '|' | '}' | '~' | '-')
}

fn is_valid_domain_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// validate_email — 20 branches: empty→invalid, no @→invalid, multiple @→invalid,
/// empty local→invalid, local>64→invalid, empty domain→invalid, domain>253→invalid,
/// quoted local→valid+quoted, local starts with dot→invalid, local ends with dot→invalid,
/// consecutive dots→invalid, invalid local char→invalid, no TLD→invalid,
/// label starts with hyphen→invalid, label ends with hyphen→invalid,
/// empty label→invalid, label>63→invalid, invalid domain char→invalid,
/// plus-addressing→valid+tag, standard→valid.
fn validate_email(email: &str) -> EmailResult {
    let invalid = |reason: &str| EmailResult {
        valid: false,
        reason: Some(reason.to_string()),
        tag: None,
        quoted: false,
    };

    if email.is_empty() {
        return invalid("empty");
    }

    let at_idx = match email.find('@') {
        Some(idx) => idx,
        None => return invalid("missing @"),
    };

    if email[at_idx + 1..].contains('@') {
        return invalid("multiple @");
    }

    let local = &email[..at_idx];
    let domain = &email[at_idx + 1..];

    if local.is_empty() {
        return invalid("empty local part");
    }
    if local.len() > LOCAL_PART_MAX {
        return invalid("local part too long");
    }
    if domain.is_empty() {
        return invalid("empty domain");
    }
    if domain.len() > DOMAIN_MAX {
        return invalid("domain too long");
    }

    if local.starts_with('"') && local.ends_with('"') && local.len() >= 2 {
        return EmailResult {
            valid: true,
            reason: None,
            tag: None,
            quoted: true,
        };
    }

    if local.starts_with('.') {
        return invalid("local starts with dot");
    }
    if local.ends_with('.') {
        return invalid("local ends with dot");
    }
    if local.contains("..") {
        return invalid("consecutive dots");
    }

    for ch in local.chars() {
        if !is_valid_local_char(ch) {
            return invalid("invalid character in local");
        }
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return invalid("domain missing TLD");
    }

    for label in &labels {
        if label.is_empty() {
            return invalid("empty domain label");
        }
        if label.len() > DOMAIN_LABEL_MAX {
            return invalid("domain label too long");
        }
        if label.starts_with('-') {
            return invalid("domain label starts with hyphen");
        }
        if label.ends_with('-') {
            return invalid("domain label ends with hyphen");
        }
        for ch in label.chars() {
            if !is_valid_domain_char(ch) {
                return invalid("invalid character in domain");
            }
        }
    }

    if let Some(plus_idx) = local.find('+') {
        return EmailResult {
            valid: true,
            reason: None,
            tag: Some(local[plus_idx + 1..].to_string()),
            quoted: false,
        };
    }

    EmailResult {
        valid: true,
        reason: None,
        tag: None,
        quoted: false,
    }
}

fn main() {
    let result = validate_email("user+tag@example.com");
    println!(
        "valid={}, reason={:?}, tag={:?}, quoted={}",
        result.valid, result.reason, result.tag, result.quoted
    );
}
