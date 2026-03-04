// Example 15: Email address validator
// Validates email addresses against a subset of RFC 5321/5322 rules.
// Exercises character-class checks, length limits, and structural validation —
// a common utility with many subtle edge cases.
//
// EXPECTED BRANCHES for validateEmail (20):
//   1. empty string                             → { valid: false, reason: "empty" }
//   2. no '@' character                         → { valid: false, reason: "missing @" }
//   3. multiple '@' characters                  → { valid: false, reason: "multiple @" }
//   4. local part empty                         → { valid: false, reason: "empty local part" }
//   5. local part > 64 chars                    → { valid: false, reason: "local part too long" }
//   6. domain empty                             → { valid: false, reason: "empty domain" }
//   7. domain > 253 chars                       → { valid: false, reason: "domain too long" }
//   8. local starts with dot                    → { valid: false, reason: "local starts with dot" }
//   9. local ends with dot                      → { valid: false, reason: "local ends with dot" }
//  10. local has consecutive dots               → { valid: false, reason: "consecutive dots" }
//  11. local has invalid character              → { valid: false, reason: "invalid character in local" }
//  12. domain has no dot (no TLD)               → { valid: false, reason: "domain missing TLD" }
//  13. domain label starts with hyphen          → { valid: false, reason: "domain label starts with hyphen" }
//  14. domain label ends with hyphen            → { valid: false, reason: "domain label ends with hyphen" }
//  15. domain label empty (consecutive dots)    → { valid: false, reason: "empty domain label" }
//  16. domain label > 63 chars                  → { valid: false, reason: "domain label too long" }
//  17. domain has invalid character             → { valid: false, reason: "invalid character in domain" }
//  18. plus-addressing detected                 → { valid: true, tag: "..." }
//  19. quoted local part (starts+ends with ")   → { valid: true, quoted: true }
//  20. standard valid address                   → { valid: true }
//
// DIFFICULTY: Hard. Many branches require precise string construction —
// specific characters at specific positions. Random generation rarely produces
// structurally valid emails, let alone ones that trigger each validation branch.

interface EmailResult {
    valid: boolean;
    reason?: string;
    tag?: string;
    quoted?: boolean;
}

const LOCAL_PART_MAX = 64;
const DOMAIN_MAX = 253;
const DOMAIN_LABEL_MAX = 63;

const VALID_LOCAL_CHARS = /^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]$/;
const VALID_DOMAIN_CHARS = /^[a-zA-Z0-9-]$/;

export function validateEmail(email: string): EmailResult {
    if (email.length === 0) {
        return { valid: false, reason: "empty" };
    }

    const atIdx = email.indexOf("@");
    if (atIdx === -1) {
        return { valid: false, reason: "missing @" };
    }
    if (email.indexOf("@", atIdx + 1) !== -1) {
        return { valid: false, reason: "multiple @" };
    }

    const local = email.slice(0, atIdx);
    const domain = email.slice(atIdx + 1);

    if (local.length === 0) {
        return { valid: false, reason: "empty local part" };
    }
    if (local.length > LOCAL_PART_MAX) {
        return { valid: false, reason: "local part too long" };
    }
    if (domain.length === 0) {
        return { valid: false, reason: "empty domain" };
    }
    if (domain.length > DOMAIN_MAX) {
        return { valid: false, reason: "domain too long" };
    }

    // Quoted local part — allows nearly any content between double quotes
    if (local.startsWith('"') && local.endsWith('"') && local.length >= 2) {
        return { valid: true, quoted: true };
    }

    // Unquoted local part validation
    if (local.startsWith(".")) {
        return { valid: false, reason: "local starts with dot" };
    }
    if (local.endsWith(".")) {
        return { valid: false, reason: "local ends with dot" };
    }
    if (local.includes("..")) {
        return { valid: false, reason: "consecutive dots" };
    }

    for (const ch of local) {
        if (!VALID_LOCAL_CHARS.test(ch)) {
            return { valid: false, reason: "invalid character in local" };
        }
    }

    // Domain validation
    const labels = domain.split(".");
    if (labels.length < 2) {
        return { valid: false, reason: "domain missing TLD" };
    }

    for (const label of labels) {
        if (label.length === 0) {
            return { valid: false, reason: "empty domain label" };
        }
        if (label.length > DOMAIN_LABEL_MAX) {
            return { valid: false, reason: "domain label too long" };
        }
        if (label.startsWith("-")) {
            return { valid: false, reason: "domain label starts with hyphen" };
        }
        if (label.endsWith("-")) {
            return { valid: false, reason: "domain label ends with hyphen" };
        }
        for (const ch of label) {
            if (!VALID_DOMAIN_CHARS.test(ch)) {
                return { valid: false, reason: "invalid character in domain" };
            }
        }
    }

    // Plus-addressing: user+tag@domain
    const plusIdx = local.indexOf("+");
    if (plusIdx !== -1) {
        return { valid: true, tag: local.slice(plusIdx + 1) };
    }

    return { valid: true };
}
