// Example 13: Roman numeral converter.
// Cascading range checks and accumulation logic with rich branching.

const ROMAN_VALUES: &[(&str, u32)] = &[
    ("M", 1000), ("CM", 900), ("D", 500), ("CD", 400),
    ("C", 100), ("XC", 90), ("L", 50), ("XL", 40),
    ("X", 10), ("IX", 9), ("V", 5), ("IV", 4), ("I", 1),
];

/// int_to_roman — 16 branches: n≤0→error, n>3999→error,
/// then cascading checks for M, CM, D, CD, C, XC, L, XL, X, IX, V, IV, I,
/// plus exhaustion (n==0).
fn int_to_roman(n: u32) -> Result<String, String> {
    if n == 0 || n > 3999 {
        return Err("out of range".to_string());
    }

    let mut result = String::new();
    let mut remaining = n;

    for &(numeral, value) in ROMAN_VALUES {
        while remaining >= value {
            result.push_str(numeral);
            remaining -= value;
        }
    }

    Ok(result)
}

use std::collections::HashMap;

/// roman_to_int — 18 branches: empty→0, invalid char→error,
/// M→1000, D→500, CM→900, CD→400, C→100, L→50, XC→90, XL→40,
/// X→10, V→5, IX→9, IV→4, I→1, subtractive pair skip,
/// end of string, lowercase uppercased.
fn roman_to_int(s: &str) -> Result<u32, String> {
    if s.is_empty() {
        return Ok(0);
    }

    let char_values: HashMap<u8, u32> = HashMap::from([
        (b'M', 1000), (b'D', 500), (b'C', 100),
        (b'L', 50), (b'X', 10), (b'V', 5), (b'I', 1),
    ]);

    let upper = s.to_uppercase();
    let bytes = upper.as_bytes();
    let mut total: u32 = 0;
    let mut i = 0;

    while i < bytes.len() {
        let val = match char_values.get(&bytes[i]) {
            Some(&v) => v,
            None => return Err("invalid roman numeral".to_string()),
        };

        let next_val = if i + 1 < bytes.len() {
            char_values.get(&bytes[i + 1]).copied().unwrap_or(0)
        } else {
            0
        };

        if next_val > val {
            total += next_val - val;
            i += 2;
        } else {
            total += val;
            i += 1;
        }
    }

    Ok(total)
}

fn main() {
    println!("{:?}", int_to_roman(1994));
    println!("{:?}", int_to_roman(0));
    println!("{:?}", roman_to_int("MCMXCIV"));
    println!("{:?}", roman_to_int("iv"));
}
