/// classify_greeting — 5 branches: "hello" → "english", "hola" → "spanish",
/// "bonjour" → "french", "ciao" → "italian", default → "unknown".
pub fn classify_greeting(s: &str) -> &'static str {
    match s {
        "hello" => "english",
        "hola" => "spanish",
        "bonjour" => "french",
        "ciao" => "italian",
        _ => "unknown",
    }
}
