// Example 2a: Classify Greeting
// Tests shatter's ability to match on string slices.
//
// EXPECTED BRANCHES (5):
//   1. s == "hello"   -> "english"
//   2. s == "hola"    -> "spanish"
//   3. s == "bonjour" -> "french"
//   4. s == "ciao"    -> "italian"
//   5. anything else  -> "unknown"
pub fn classify_greeting(s: &str) -> &'static str {
    match s {
        "hello" => "english",
        "hola" => "spanish",
        "bonjour" => "french",
        "ciao" => "italian",
        _ => "unknown",
    }
}

// Example 2b: Classify File Extension
// Tests shatter's ability to handle string suffix matching.
//
// EXPECTED BRANCHES (5):
//   1. ends with ".rs"   -> "rust"
//   2. ends with ".ts"   -> "typescript"
//   3. ends with ".go"   -> "go"
//   4. ends with ".py"   -> "python"
//   5. anything else     -> "unknown"
//
// DIFFICULTY: Medium. Requires generating strings with specific suffixes.
pub fn classify_extension(filename: &str) -> &'static str {
    if filename.ends_with(".rs") {
        "rust"
    } else if filename.ends_with(".ts") {
        "typescript"
    } else if filename.ends_with(".go") {
        "go"
    } else if filename.ends_with(".py") {
        "python"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_greeting_english() {
        assert_eq!(classify_greeting("hello"), "english");
    }

    #[test]
    fn test_classify_greeting_spanish() {
        assert_eq!(classify_greeting("hola"), "spanish");
    }

    #[test]
    fn test_classify_greeting_french() {
        assert_eq!(classify_greeting("bonjour"), "french");
    }

    #[test]
    fn test_classify_greeting_italian() {
        assert_eq!(classify_greeting("ciao"), "italian");
    }

    #[test]
    fn test_classify_greeting_unknown() {
        assert_eq!(classify_greeting("hey"), "unknown");
    }

    #[test]
    fn test_classify_extension_rust() {
        assert_eq!(classify_extension("main.rs"), "rust");
    }

    #[test]
    fn test_classify_extension_typescript() {
        assert_eq!(classify_extension("app.ts"), "typescript");
    }

    #[test]
    fn test_classify_extension_go() {
        assert_eq!(classify_extension("main.go"), "go");
    }

    #[test]
    fn test_classify_extension_python() {
        assert_eq!(classify_extension("script.py"), "python");
    }

    #[test]
    fn test_classify_extension_unknown() {
        assert_eq!(classify_extension("readme.md"), "unknown");
    }
}
