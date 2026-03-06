//! Structure-aware string mutation strategies.
//!
//! Supplements the char-level operators in `input_gen::mutate_string` with
//! higher-level mutations that exploit string structure: delimiters, domain
//! fragments, weighted character distributions, and unicode edge cases.

use rand::Rng;

// ---------------------------------------------------------------------------
// Pattern fragment dictionaries — domain-relevant substrings for insertion
// ---------------------------------------------------------------------------

const EMAIL_FRAGMENTS: &[&str] = &["@", ".com", ".org", ".net", "user@", "@example.com", "admin@"];
const URL_FRAGMENTS: &[&str] = &["://", "https://", "http://", "/api/", "?key=", "&val=", "#anchor", "localhost"];
const PATH_FRAGMENTS: &[&str] = &["/", "../", "./", ".txt", ".json", ".csv", "\\"];
const DELIMITER_FRAGMENTS: &[&str] = &[",", ";", "\t", "\n", "|", ":", "=", "&"];
const NUMERIC_FRAGMENTS: &[&str] = &["0", "-1", "999", "0.0", "NaN", "Infinity", "-0"];
const WHITESPACE_FRAGMENTS: &[&str] = &[" ", "  ", "\t", "\r\n", "\n", "\r"];

const ALL_PATTERN_GROUPS: &[&[&str]] = &[
    EMAIL_FRAGMENTS,
    URL_FRAGMENTS,
    PATH_FRAGMENTS,
    DELIMITER_FRAGMENTS,
    NUMERIC_FRAGMENTS,
    WHITESPACE_FRAGMENTS,
];

/// Characters that appear frequently at domain boundaries and are
/// more likely to trigger interesting branch conditions than uniform ASCII.
const WEIGHTED_SPECIAL_CHARS: &[char] = &[
    '@', '.', '-', '_', '/', ':', '?', '=', '&', '#', '+', '%', '!', '*', '~',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
];

/// Unicode values useful for exercising boundary/security code paths.
const UNICODE_INTERESTING: &[char] = &[
    '\u{200B}', // zero-width space
    '\u{200C}', // zero-width non-joiner
    '\u{200D}', // zero-width joiner
    '\u{FEFF}', // BOM / zero-width no-break space
    '\u{202E}', // RTL override
    '\u{0301}', // combining acute accent
    '\u{0000}', // null
    '\u{FFFD}', // replacement character
    '\u{4E16}', // CJK: "world"
    '\u{1F600}', // emoji: grinning face
];

/// Common structural delimiters used by `split_on_delimiter`.
const STRUCTURE_DELIMITERS: &[char] = &['.', '/', '@', '-', '_', ':', ',', ';', '|', '='];

/// Number of structure-aware mutation strategies.
const STRATEGY_COUNT: u8 = 7;

// ---------------------------------------------------------------------------
// Top-level dispatcher
// ---------------------------------------------------------------------------

/// Apply a random structure-aware mutation to `s`.
///
/// Returns the original string unchanged only if `s` is empty and the chosen
/// strategy cannot operate on empty input.
pub fn mutate_structure_aware(s: &str, rng: &mut impl Rng) -> String {
    let strategy = rng.random_range(0..STRATEGY_COUNT);
    match strategy {
        0 => mutate_multi_point(s, rng),
        1 => mutate_pattern_insert(s, rng),
        2 => mutate_weighted_char(s, rng),
        3 => mutate_unicode_insert(s, rng),
        4 => mutate_unicode_boundary(s, rng),
        5 => mutate_swap_segments(s, rng),
        6 => mutate_duplicate_segment(s, rng),
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Strategy implementations
// ---------------------------------------------------------------------------

/// Replace 2–4 characters at random positions with random printable ASCII.
///
/// More disruptive than single-char substitution; better at simultaneously
/// breaking multiple validation checks.
pub fn mutate_multi_point(s: &str, rng: &mut impl Rng) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let num_points = rng.random_range(2..=4.min(chars.len()));
    let mut mutated = chars.clone();
    for _ in 0..num_points {
        let idx = rng.random_range(0..chars.len());
        mutated[idx] = rng.random_range(b' '..=b'~') as char;
    }
    mutated.into_iter().collect()
}

/// Insert a domain-relevant fragment (email, URL, path, delimiter, etc.)
/// at a random character boundary.
pub fn mutate_pattern_insert(s: &str, rng: &mut impl Rng) -> String {
    let group = ALL_PATTERN_GROUPS[rng.random_range(0..ALL_PATTERN_GROUPS.len())];
    let fragment = group[rng.random_range(0..group.len())];
    let chars: Vec<char> = s.chars().collect();
    let pos = rng.random_range(0..=chars.len());
    let mut result = String::with_capacity(s.len() + fragment.len());
    for (i, ch) in chars.iter().enumerate() {
        if i == pos {
            result.push_str(fragment);
        }
        result.push(*ch);
    }
    if pos == chars.len() {
        result.push_str(fragment);
    }
    result
}

/// Replace a random character with one drawn from a weighted distribution
/// favoring domain-relevant characters (digits, special chars like @, ., /).
///
/// 60% chance of a special/digit char, 40% uniform printable ASCII.
pub fn mutate_weighted_char(s: &str, rng: &mut impl Rng) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let idx = rng.random_range(0..chars.len());
    let new_char = if rng.random_range(0..5) < 3 {
        WEIGHTED_SPECIAL_CHARS[rng.random_range(0..WEIGHTED_SPECIAL_CHARS.len())]
    } else {
        rng.random_range(b' '..=b'~') as char
    };
    let mut result: Vec<char> = chars;
    result[idx] = new_char;
    result.into_iter().collect()
}

/// Insert an interesting unicode character at a random position.
///
/// Exercises unicode handling, security checks (null bytes, RTL override,
/// zero-width chars), and multi-byte boundary correctness.
pub fn mutate_unicode_insert(s: &str, rng: &mut impl Rng) -> String {
    let uc = UNICODE_INTERESTING[rng.random_range(0..UNICODE_INTERESTING.len())];
    let chars: Vec<char> = s.chars().collect();
    let pos = rng.random_range(0..=chars.len());
    let mut result = String::with_capacity(s.len() + uc.len_utf8());
    for (i, ch) in chars.iter().enumerate() {
        if i == pos {
            result.push(uc);
        }
        result.push(*ch);
    }
    if pos == chars.len() {
        result.push(uc);
    }
    result
}

/// Insert an interesting unicode character at a structural boundary:
/// start, end, or adjacent to an existing special character.
pub fn mutate_unicode_boundary(s: &str, rng: &mut impl Rng) -> String {
    let uc = UNICODE_INTERESTING[rng.random_range(0..UNICODE_INTERESTING.len())];
    if s.is_empty() {
        return uc.to_string();
    }
    let chars: Vec<char> = s.chars().collect();

    // Find positions adjacent to special chars, plus start/end
    let mut boundary_positions: Vec<usize> = vec![0, chars.len()];
    for (i, ch) in chars.iter().enumerate() {
        if STRUCTURE_DELIMITERS.contains(ch) {
            boundary_positions.push(i);
            if i < chars.len() {
                boundary_positions.push(i + 1);
            }
        }
    }
    boundary_positions.sort_unstable();
    boundary_positions.dedup();

    let pos = boundary_positions[rng.random_range(0..boundary_positions.len())];
    let mut result = String::with_capacity(s.len() + uc.len_utf8());
    for (i, ch) in chars.iter().enumerate() {
        if i == pos {
            result.push(uc);
        }
        result.push(*ch);
    }
    if pos == chars.len() {
        result.push(uc);
    }
    result
}

/// Split on the first matching structural delimiter, then swap two segments.
///
/// For "user@example.com" split on '@' → ["user", "example.com"],
/// result: "example.com@user". Preserves structure while rearranging content.
pub fn mutate_swap_segments(s: &str, rng: &mut impl Rng) -> String {
    if let Some((delim, segments)) = split_on_best_delimiter(s) {
        if segments.len() < 2 {
            return s.to_string();
        }
        let mut segments = segments;
        let a = rng.random_range(0..segments.len());
        let mut b = rng.random_range(0..segments.len());
        // Ensure we pick two different indices when possible
        if segments.len() > 1 && a == b {
            b = (a + 1) % segments.len();
        }
        segments.swap(a, b);
        segments.join(&delim.to_string())
    } else {
        s.to_string()
    }
}

/// Split on a structural delimiter, then duplicate a random segment.
///
/// "a.b.c" → "a.b.b.c" — useful for triggering length checks, off-by-one
/// errors in parsers, and unexpected repetition.
pub fn mutate_duplicate_segment(s: &str, rng: &mut impl Rng) -> String {
    if let Some((delim, segments)) = split_on_best_delimiter(s) {
        if segments.is_empty() {
            return s.to_string();
        }
        let mut segments = segments;
        let idx = rng.random_range(0..segments.len());
        let dup = segments[idx].to_string();
        segments.insert(idx + 1, dup);
        segments.join(&delim.to_string())
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the delimiter that produces the most segments (at least 2).
/// Returns `None` if no delimiter splits the string into 2+ parts.
fn split_on_best_delimiter(s: &str) -> Option<(char, Vec<String>)> {
    let mut best: Option<(char, Vec<String>)> = None;
    let mut best_count = 1usize;
    for &d in STRUCTURE_DELIMITERS {
        let parts: Vec<String> = s.split(d).map(|p| p.to_string()).collect();
        if parts.len() > best_count {
            best_count = parts.len();
            best = Some((d, parts));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(42)
    }

    // -------------------------------------------------------------------
    // mutate_multi_point
    // -------------------------------------------------------------------

    #[test]
    fn multi_point_empty_returns_empty() {
        assert_eq!(mutate_multi_point("", &mut rng()), "");
    }

    #[test]
    fn multi_point_short_string_changes_chars() {
        let result = mutate_multi_point("ab", &mut rng());
        assert_eq!(result.chars().count(), 2);
        // At least one char should differ (overwhelmingly likely with seed 42)
        assert_ne!(result, "ab");
    }

    #[test]
    fn multi_point_preserves_length() {
        let input = "hello world";
        let result = mutate_multi_point(input, &mut rng());
        assert_eq!(result.chars().count(), input.chars().count());
    }

    #[test]
    fn multi_point_unicode_preserves_count() {
        let input = "hello\u{4E16}\u{1F600}";
        let result = mutate_multi_point(input, &mut rng());
        // Char count preserved (replacements are ASCII, so count stays same)
        assert_eq!(result.chars().count(), input.chars().count());
    }

    // -------------------------------------------------------------------
    // mutate_pattern_insert
    // -------------------------------------------------------------------

    #[test]
    fn pattern_insert_empty_produces_fragment() {
        let result = mutate_pattern_insert("", &mut rng());
        assert!(!result.is_empty());
    }

    #[test]
    fn pattern_insert_grows_string() {
        let input = "test";
        let result = mutate_pattern_insert(input, &mut rng());
        assert!(result.len() > input.len());
    }

    #[test]
    fn pattern_insert_contains_original_chars() {
        let input = "abc";
        let result = mutate_pattern_insert(input, &mut rng());
        // All original chars should still be present in order
        let mut chars = result.chars();
        for c in input.chars() {
            loop {
                match chars.next() {
                    Some(found) if found == c => break,
                    Some(_) => continue,
                    None => panic!("original char '{c}' not found in result '{result}'"),
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // mutate_weighted_char
    // -------------------------------------------------------------------

    #[test]
    fn weighted_char_empty_returns_empty() {
        assert_eq!(mutate_weighted_char("", &mut rng()), "");
    }

    #[test]
    fn weighted_char_changes_one_char() {
        let input = "aaaa";
        let result = mutate_weighted_char(input, &mut rng());
        assert_eq!(result.chars().count(), 4);
        let diffs: usize = input
            .chars()
            .zip(result.chars())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(diffs, 1);
    }

    #[test]
    fn weighted_char_tends_toward_special() {
        // Over many iterations, weighted selection should produce special chars
        // more often than uniform would
        let mut rng = SmallRng::seed_from_u64(0);
        let mut special_count = 0;
        for _ in 0..100 {
            let result = mutate_weighted_char("aaaa", &mut rng);
            let replaced = result.chars().find(|c| *c != 'a');
            if let Some(c) = replaced {
                if WEIGHTED_SPECIAL_CHARS.contains(&c) {
                    special_count += 1;
                }
            }
        }
        // With 60% weight, expect ~60 special chars out of 100
        assert!(special_count > 30, "expected >30 special chars, got {special_count}");
    }

    // -------------------------------------------------------------------
    // mutate_unicode_insert
    // -------------------------------------------------------------------

    #[test]
    fn unicode_insert_empty_produces_single_char() {
        let result = mutate_unicode_insert("", &mut rng());
        assert_eq!(result.chars().count(), 1);
        assert!(UNICODE_INTERESTING.contains(&result.chars().next().expect("non-empty")));
    }

    #[test]
    fn unicode_insert_adds_one_char() {
        let input = "test";
        let result = mutate_unicode_insert(input, &mut rng());
        assert_eq!(result.chars().count(), input.chars().count() + 1);
    }

    #[test]
    fn unicode_insert_valid_utf8() {
        // Rust strings are always valid UTF-8, but verify round-trip
        let input = "hello\u{4E16}world";
        let result = mutate_unicode_insert(input, &mut rng());
        assert!(result.is_ascii() || !result.is_empty()); // always valid
        assert_eq!(result.chars().count(), input.chars().count() + 1);
    }

    // -------------------------------------------------------------------
    // mutate_unicode_boundary
    // -------------------------------------------------------------------

    #[test]
    fn unicode_boundary_empty_produces_single_char() {
        let result = mutate_unicode_boundary("", &mut rng());
        assert_eq!(result.chars().count(), 1);
    }

    #[test]
    fn unicode_boundary_inserts_near_delimiters() {
        let input = "user@example.com";
        let result = mutate_unicode_boundary(input, &mut rng());
        assert_eq!(result.chars().count(), input.chars().count() + 1);
    }

    #[test]
    fn unicode_boundary_no_delimiter_uses_edges() {
        let input = "abcdef";
        let result = mutate_unicode_boundary(input, &mut rng());
        assert_eq!(result.chars().count(), input.chars().count() + 1);
        // Should insert at start or end (only boundary positions)
        let first = result.chars().next().expect("non-empty");
        let last = result.chars().last().expect("non-empty");
        let inserted_at_edge =
            UNICODE_INTERESTING.contains(&first) || UNICODE_INTERESTING.contains(&last);
        assert!(inserted_at_edge, "expected insertion at edge for no-delimiter string");
    }

    // -------------------------------------------------------------------
    // mutate_swap_segments
    // -------------------------------------------------------------------

    #[test]
    fn swap_segments_no_delimiter_returns_original() {
        assert_eq!(mutate_swap_segments("hello", &mut rng()), "hello");
    }

    #[test]
    fn swap_segments_email() {
        let input = "user@example.com";
        let result = mutate_swap_segments(input, &mut rng());
        // Should be a rearrangement of the segments
        assert_ne!(result, input);
        // Same total content (excluding delimiters — segments are swapped not removed)
        let orig_parts: Vec<&str> = input.split('.').collect();
        let result_parts: Vec<&str> = result.split('.').collect();
        assert_eq!(orig_parts.len(), result_parts.len());
    }

    #[test]
    fn swap_segments_path() {
        let input = "/usr/local/bin";
        let result = mutate_swap_segments(input, &mut rng());
        // Segments are rearranged
        let orig_parts: Vec<&str> = input.split('/').collect();
        let result_parts: Vec<&str> = result.split('/').collect();
        assert_eq!(orig_parts.len(), result_parts.len());
    }

    #[test]
    fn swap_segments_single_delimiter_two_parts() {
        let input = "a@b";
        let result = mutate_swap_segments(input, &mut rng());
        assert_eq!(result, "b@a");
    }

    // -------------------------------------------------------------------
    // mutate_duplicate_segment
    // -------------------------------------------------------------------

    #[test]
    fn duplicate_segment_no_delimiter_returns_original() {
        assert_eq!(mutate_duplicate_segment("hello", &mut rng()), "hello");
    }

    #[test]
    fn duplicate_segment_grows_by_one() {
        let input = "a.b.c";
        let result = mutate_duplicate_segment(input, &mut rng());
        let orig_count = input.split('.').count();
        let result_count = result.split('.').count();
        assert_eq!(result_count, orig_count + 1);
    }

    #[test]
    fn duplicate_segment_preserves_content() {
        let input = "user@example";
        let result = mutate_duplicate_segment(input, &mut rng());
        // Result should contain all original segments plus one duplicate
        assert!(result.contains("user") && result.contains("example"));
    }

    // -------------------------------------------------------------------
    // split_on_best_delimiter
    // -------------------------------------------------------------------

    #[test]
    fn best_delimiter_picks_most_segments() {
        // "a.b.c@d" — '.' produces 3 segments, '@' produces 2
        let (delim, parts) = split_on_best_delimiter("a.b.c@d").expect("should split");
        assert_eq!(delim, '.');
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn best_delimiter_none_for_no_delimiters() {
        assert!(split_on_best_delimiter("hello").is_none());
    }

    // -------------------------------------------------------------------
    // mutate_structure_aware (dispatcher)
    // -------------------------------------------------------------------

    #[test]
    fn dispatcher_handles_empty_string() {
        let mut rng = SmallRng::seed_from_u64(0);
        // Run all strategies — none should panic on empty input
        for seed in 0..100 {
            let mut r = SmallRng::seed_from_u64(seed);
            let _ = mutate_structure_aware("", &mut r);
        }
    }

    #[test]
    fn dispatcher_handles_ascii() {
        let mut rng = SmallRng::seed_from_u64(0);
        for _ in 0..50 {
            let _result = mutate_structure_aware("hello world", &mut rng);
        }
    }

    #[test]
    fn dispatcher_handles_unicode() {
        let mut rng = SmallRng::seed_from_u64(0);
        let input = "hello\u{4E16}\u{754C}\u{1F600}test";
        for _ in 0..50 {
            let _ = mutate_structure_aware(input, &mut rng);
        }
    }

    #[test]
    fn dispatcher_handles_emoji_only() {
        let mut rng = SmallRng::seed_from_u64(0);
        let input = "\u{1F600}\u{1F601}\u{1F602}";
        for _ in 0..50 {
            let _ = mutate_structure_aware(input, &mut rng);
        }
    }

    #[test]
    fn dispatcher_produces_variety() {
        // With different seeds, we should see different results
        let input = "user@example.com";
        let mut results = std::collections::HashSet::new();
        for seed in 0..100 {
            let mut r = SmallRng::seed_from_u64(seed);
            results.insert(mutate_structure_aware(input, &mut r));
        }
        assert!(results.len() > 5, "expected variety, got {} unique results", results.len());
    }

    #[test]
    fn dispatcher_covers_all_strategies() {
        // Verify that over many seeds, all 7 strategies produce output
        // (implicitly tested by the variety test, but this is more explicit)
        let input = "a.b/c@d-e_f";
        let mut results = Vec::new();
        for seed in 0..200 {
            let mut r = SmallRng::seed_from_u64(seed);
            results.push(mutate_structure_aware(input, &mut r));
        }
        // Should have many distinct results (7 strategies × varied randomness)
        let unique: std::collections::HashSet<_> = results.iter().collect();
        assert!(unique.len() > 15, "expected >15 unique results, got {}", unique.len());
    }
}
