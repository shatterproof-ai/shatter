// Example 18: Accept-Language negotiation.
// Adapted from jshttp/negotiator (MIT): https://github.com/jshttp/negotiator
// Header parsing, q-value ranking, wildcard handling, and exact-versus-prefix matching.
//
// EXPECTED BRANCHES for negotiate_language (13):
//   1. supported list empty                      -> no-supported
//   2. header empty                              -> default to first supported
//   3. all ranges invalid                        -> default to first supported
//   4. q=0 range excluded                        -> ignored during selection
//   5. wildcard "*" matches                      -> first supported
//   6. exact full-tag match                      -> exact
//   7. generic range matches specific supported  -> prefix
//   8. specific range falls back to generic      -> fallback
//   9. higher q beats lower q                    -> highest quality wins
//  10. specificity breaks q ties                 -> exact beats prefix/fallback
//  11. original order breaks full ties           -> earlier range wins
//  12. invalid q parameter ignored               -> range dropped
//  13. no match after parsing                    -> no-match
//
// DIFFICULTY: Hard. The solver must generate structured header strings with
// weighted preferences and language tags that interact with the supported list.

#[derive(Debug, Clone)]
struct LanguagePreference {
    tag: String,
    primary: String,
    region: String,
    q: f64,
    specificity: u8,
    order: usize,
}

#[derive(Debug)]
struct LanguageResult {
    selected: Option<String>,
    reason: String,
    matched_range: Option<String>,
    quality: f64,
}

fn parse_language_preference(part: &str, order: usize) -> Option<LanguagePreference> {
    let trimmed = part.trim();
    if trimmed.is_empty() {
        return None;
    }

    let segments: Vec<&str> = trimmed.split(';').collect();
    let normalized = segments[0].trim().to_ascii_lowercase();
    if normalized != "*" {
        let pieces: Vec<&str> = normalized.split('-').collect();
        if pieces.is_empty() || pieces.len() > 2 {
            return None;
        }
        for piece in &pieces {
            if piece.is_empty() || !piece.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                return None;
            }
        }
    }

    let mut q = 1.0;
    for segment in segments.iter().skip(1) {
        let mut parts = segment.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if key != "q" {
            continue;
        }
        let parsed = value.parse::<f64>().ok()?;
        if !(0.0..=1.0).contains(&parsed) {
            return None;
        }
        q = parsed;
    }

    if normalized == "*" {
        return Some(LanguagePreference {
            tag: normalized,
            primary: "*".to_string(),
            region: String::new(),
            q,
            specificity: 0,
            order,
        });
    }

    let mut pieces = normalized.splitn(2, '-');
    let primary = pieces.next().unwrap_or("").to_string();
    let region = pieces.next().unwrap_or("").to_string();

    Some(LanguagePreference {
        tag: normalized,
        primary,
        region: region.clone(),
        q,
        specificity: if region.is_empty() { 1 } else { 2 },
        order,
    })
}

fn negotiate_language(header: &str, supported: &[&str]) -> LanguageResult {
    if supported.is_empty() {
        return LanguageResult {
            selected: None,
            reason: "no-supported".to_string(),
            matched_range: None,
            quality: 0.0,
        };
    }

    if header.trim().is_empty() {
        return LanguageResult {
            selected: Some(supported[0].to_string()),
            reason: "default".to_string(),
            matched_range: None,
            quality: 1.0,
        };
    }

    let mut preferences: Vec<LanguagePreference> = header
        .split(',')
        .enumerate()
        .filter_map(|(i, part)| parse_language_preference(part, i))
        .collect();

    if preferences.is_empty() {
        return LanguageResult {
            selected: Some(supported[0].to_string()),
            reason: "default".to_string(),
            matched_range: None,
            quality: 1.0,
        };
    }

    preferences.sort_by(|a, b| {
        b.q.total_cmp(&a.q)
            .then(b.specificity.cmp(&a.specificity))
            .then(a.order.cmp(&b.order))
    });

    let normalized_supported: Vec<String> = supported
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect();

    for pref in &preferences {
        if pref.q == 0.0 {
            continue;
        }

        if pref.tag == "*" {
            return LanguageResult {
                selected: Some(supported[0].to_string()),
                reason: "wildcard".to_string(),
                matched_range: Some(pref.tag.clone()),
                quality: pref.q,
            };
        }

        for (index, candidate) in normalized_supported.iter().enumerate() {
            if candidate == &pref.tag {
                return LanguageResult {
                    selected: Some(supported[index].to_string()),
                    reason: "exact".to_string(),
                    matched_range: Some(pref.tag.clone()),
                    quality: pref.q,
                };
            }
        }

        for (index, candidate) in normalized_supported.iter().enumerate() {
            if candidate.starts_with(&(pref.primary.clone() + "-")) {
                return LanguageResult {
                    selected: Some(supported[index].to_string()),
                    reason: "prefix".to_string(),
                    matched_range: Some(pref.tag.clone()),
                    quality: pref.q,
                };
            }
        }

        if !pref.region.is_empty() {
            for (index, candidate) in normalized_supported.iter().enumerate() {
                if candidate == &pref.primary {
                    return LanguageResult {
                        selected: Some(supported[index].to_string()),
                        reason: "fallback".to_string(),
                        matched_range: Some(pref.tag.clone()),
                        quality: pref.q,
                    };
                }
            }
        }
    }

    LanguageResult {
        selected: None,
        reason: "no-match".to_string(),
        matched_range: None,
        quality: 0.0,
    }
}

fn main() {
    let result = negotiate_language("fr-CA, fr;q=0.9, en;q=0.5", &["en", "fr-FR", "fr"]);
    println!(
        "selected={:?} reason={} matched_range={:?} quality={}",
        result.selected, result.reason, result.matched_range, result.quality
    );
}
