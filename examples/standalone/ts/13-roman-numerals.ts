// Example 13: Roman numeral converter
// Converts integers to Roman numeral strings. Exercises cascading range checks
// and accumulation logic — a recognizable utility with rich branching.
//
// EXPECTED BRANCHES for intToRoman (16):
//   1. n <= 0                                   → throws Error("out of range")
//   2. n > 3999                                 → throws Error("out of range")
//   3. n >= 1000                                → prepends "M", subtracts 1000
//   4. n >= 900                                 → prepends "CM", subtracts 900
//   5. n >= 500                                 → prepends "D", subtracts 500
//   6. n >= 400                                 → prepends "CD", subtracts 400
//   7. n >= 100                                 → prepends "C", subtracts 100
//   8. n >= 90                                  → prepends "XC", subtracts 90
//   9. n >= 50                                  → prepends "L", subtracts 50
//  10. n >= 40                                  → prepends "XL", subtracts 40
//  11. n >= 10                                  → prepends "X", subtracts 10
//  12. n >= 9                                   → prepends "IX", subtracts 9
//  13. n >= 5                                   → prepends "V", subtracts 5
//  14. n >= 4                                   → prepends "IV", subtracts 4
//  15. n >= 1                                   → prepends "I", subtracts 1
//  16. n == 0 (exhausted)                       → returns accumulated string
//
// EXPECTED BRANCHES for romanToInt (18):
//   1. empty string                             → 0
//   2. invalid character                        → throws Error("invalid roman numeral")
//   3. char 'M'                                 → adds 1000
//   4. char 'D'                                 → adds 500
//   5. char 'C' followed by 'M'                → adds 900 (subtractive)
//   6. char 'C' followed by 'D'                → adds 400 (subtractive)
//   7. char 'C' (normal)                        → adds 100
//   8. char 'L'                                 → adds 50
//   9. char 'X' followed by 'C'                → adds 90 (subtractive)
//  10. char 'X' followed by 'L'                → adds 40 (subtractive)
//  11. char 'X' (normal)                        → adds 10
//  12. char 'V'                                 → adds 5
//  13. char 'I' followed by 'X'                → adds 9 (subtractive)
//  14. char 'I' followed by 'V'                → adds 4 (subtractive)
//  15. char 'I' (normal)                        → adds 1
//  16. subtractive pair consumed                → skip next char
//  17. end of string                            → return total
//  18. lowercase input                          → uppercased before processing
//
// DIFFICULTY: Medium. The cascading range checks are straightforward for
// numeric constraint solving, but romanToInt requires string-content reasoning.

const ROMAN_VALUES: [string, number][] = [
    ["M", 1000], ["CM", 900], ["D", 500], ["CD", 400],
    ["C", 100], ["XC", 90], ["L", 50], ["XL", 40],
    ["X", 10], ["IX", 9], ["V", 5], ["IV", 4], ["I", 1],
];

export function intToRoman(n: number): string {
    if (n <= 0 || n > 3999) {
        throw new Error("out of range");
    }

    let result = "";
    let remaining = n;

    for (const [numeral, value] of ROMAN_VALUES) {
        while (remaining >= value) {
            result += numeral;
            remaining -= value;
        }
    }

    return result;
}

const CHAR_VALUES: Record<string, number> = {
    M: 1000, D: 500, C: 100, L: 50, X: 10, V: 5, I: 1,
};

export function romanToInt(s: string): number {
    if (s.length === 0) {
        return 0;
    }

    const upper = s.toUpperCase();
    let total = 0;
    let i = 0;

    while (i < upper.length) {
        const ch = upper[i];
        const val = CHAR_VALUES[ch];
        if (val === undefined) {
            throw new Error("invalid roman numeral");
        }

        const next = i + 1 < upper.length ? CHAR_VALUES[upper[i + 1]] : 0;
        if (next !== undefined && next > val) {
            total += next - val;
            i += 2;
        } else {
            total += val;
            i += 1;
        }
    }

    return total;
}
