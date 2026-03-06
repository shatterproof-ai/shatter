// Example 2: String manipulation with conditionals.
// Tests shatter's ability to reason about string properties.

/// classify_string — 6 branches: empty→"empty", len==1→"single-char",
/// starts with "http"→"url", contains '@'→"email-like",
/// all digits→"numeric", else→"text".
fn classify_string(s: &str) -> &'static str {
    if s.is_empty() {
        return "empty";
    }
    if s.len() == 1 {
        return "single-char";
    }
    if s.starts_with("http") {
        return "url";
    }
    if s.contains('@') {
        return "email-like";
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return "numeric";
    }
    "text"
}

/// transform_string — 6 branches: mode "upper"→uppercased, "lower"→lowercased,
/// "reverse"→reversed, "repeat"+count≤0→"", "repeat"+count>0→repeated, else→unchanged.
fn transform_string(input: &str, mode: &str, count: usize) -> String {
    match mode {
        "upper" => input.to_uppercase(),
        "lower" => input.to_lowercase(),
        "reverse" => input.chars().rev().collect(),
        "repeat" => {
            if count == 0 {
                String::new()
            } else {
                input.repeat(count)
            }
        }
        _ => input.to_string(),
    }
}

fn main() {
    println!("{}", classify_string(""));
    println!("{}", classify_string("x"));
    println!("{}", classify_string("https://example.com"));
    println!("{}", classify_string("user@host"));
    println!("{}", classify_string("12345"));
    println!("{}", classify_string("hello world"));
    println!("{}", transform_string("hello", "upper", 0));
    println!("{}", transform_string("HELLO", "lower", 0));
    println!("{}", transform_string("abc", "reverse", 0));
    println!("{}", transform_string("ha", "repeat", 3));
}
