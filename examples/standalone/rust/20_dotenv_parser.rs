// Example 20: dotenv parser.
// Adapted from motdotla/dotenv (BSD-2-Clause): https://github.com/motdotla/dotenv
// Line parsing, export-prefix handling, quoted values, comments, and multiline quotes.
//
// EXPECTED BRANCHES for parse_dotenv (16):
//   1. empty input                               -> empty result
//   2. blank lines ignored                       -> skipped
//   3. comment lines ignored                     -> skipped
//   4. invalid line without separator            -> warning emitted
//   5. invalid key characters                    -> warning emitted
//   6. export prefix stripped                    -> key parsed
//   7. '=' separator parsed                      -> value assigned
//   8. ':' separator parsed                      -> value assigned
//   9. empty value allowed                       -> empty string
//  10. unquoted values trimmed                   -> whitespace removed
//  11. inline comment on unquoted value removed  -> comment stripped
//  12. single-quoted value preserved             -> escapes left literal
//  13. double-quoted value expands \n and \r     -> escaped newlines expanded
//  14. hash inside quoted value preserved        -> not treated as comment
//  15. multiline quoted value consumed           -> joins following lines
//  16. unterminated quoted value                 -> warning emitted
//
// DIFFICULTY: Hard. The solver must craft line-oriented text with quotes,
// separators, comments, and multiline structure that interact precisely.

use std::collections::BTreeMap;

struct DotenvResult {
    values: BTreeMap<String, String>,
    warnings: Vec<String>,
}

fn find_dotenv_separator(line: &str) -> Option<usize> {
    match (line.find('='), line.find(':')) {
        (Some(eq), Some(colon)) => Some(eq.min(colon)),
        (Some(eq), None) => Some(eq),
        (None, Some(colon)) => Some(colon),
        (None, None) => None,
    }
}

fn strip_inline_dotenv_comment(value: &str) -> String {
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;

    for (idx, ch) in value.char_indices() {
        match ch {
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '#' if !in_single && !in_double && !in_backtick => {
                return value[..idx].to_string();
            }
            _ => {}
        }
    }

    value.to_string()
}

fn valid_dotenv_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
}

fn parse_dotenv(src: &str) -> DotenvResult {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut values = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let mut line = lines[i].trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if line.starts_with("export ") {
            line = line["export ".len()..].trim().to_string();
        }

        let Some(sep) = find_dotenv_separator(&line) else {
            warnings.push(format!("line {}: missing separator", i + 1));
            i += 1;
            continue;
        };

        let key = line[..sep].trim();
        if !valid_dotenv_key(key) {
            warnings.push(format!("line {}: invalid key", i + 1));
            i += 1;
            continue;
        }

        let raw_value = line[sep + 1..].trim_start();
        if raw_value.is_empty() {
            values.insert(key.to_string(), String::new());
            i += 1;
            continue;
        }

        let quote = raw_value.chars().next().unwrap_or_default();
        if matches!(quote, '\'' | '"' | '`') {
            let mut body = raw_value[1..].to_string();
            let mut closed = false;

            loop {
                if let Some(idx) = body.find(quote) {
                    body = body[..idx].to_string();
                    closed = true;
                    break;
                }
                i += 1;
                if i >= lines.len() {
                    break;
                }
                body.push('\n');
                body.push_str(lines[i]);
            }

            if !closed {
                warnings.push(format!("line {}: unterminated quote", i + 1));
                i += 1;
                continue;
            }

            if quote == '"' {
                body = body.replace("\\n", "\n").replace("\\r", "\r");
            }

            values.insert(key.to_string(), body);
            i += 1;
            continue;
        }

        let value = strip_inline_dotenv_comment(raw_value).trim().to_string();
        values.insert(key.to_string(), value);
        i += 1;
    }

    DotenvResult { values, warnings }
}

fn main() {
    let result = parse_dotenv("export NAME=shatter\nEMPTY=\nQUOTE=\"hello\\nworld\"");
    println!("values={:?} warnings={:?}", result.values, result.warnings);
}
