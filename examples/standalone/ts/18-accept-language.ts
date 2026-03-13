// Example 18: Accept-Language negotiation
// Adapted from jshttp/negotiator (MIT): https://github.com/jshttp/negotiator
// Exercises header parsing, q-value ranking, wildcard handling, and
// exact-versus-prefix matching across a supported language set.
//
// EXPECTED BRANCHES for negotiateLanguage (13):
//   1. supported list empty                      -> { selected: null, reason: "no-supported" }
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
//  13. no match after parsing                    -> { selected: null, reason: "no-match" }
//
// DIFFICULTY: Hard. The solver must generate structured header strings with
// weighted preferences and language tags that interact with the supported list.

interface LanguagePreference {
    tag: string;
    primary: string;
    region: string;
    q: number;
    specificity: number;
    order: number;
}

interface LanguageResult {
    selected: string | null;
    reason: string;
    matchedRange: string | null;
    quality: number;
}

function parsePreference(part: string, order: number): LanguagePreference | null {
    const trimmed = part.trim();
    if (trimmed.length === 0) {
        return null;
    }

    const segments = trimmed.split(";");
    const tag = segments[0].trim();
    if (!/^(\*|[a-zA-Z]{1,8}(?:-[a-zA-Z0-9]{1,8})?)$/.test(tag)) {
        return null;
    }

    let q = 1;
    for (let i = 1; i < segments.length; i++) {
        const [key, value] = segments[i].split("=", 2).map(s => s.trim());
        if (key === "q") {
            const parsed = Number(value);
            if (!isFinite(parsed) || parsed < 0 || parsed > 1) {
                return null;
            }
            q = parsed;
        }
    }

    const normalized = tag.toLowerCase();
    if (normalized === "*") {
        return {
            tag: normalized,
            primary: "*",
            region: "",
            q,
            specificity: 0,
            order,
        };
    }

    const [primary, region = ""] = normalized.split("-", 2);
    return {
        tag: normalized,
        primary,
        region,
        q,
        specificity: region.length > 0 ? 2 : 1,
        order,
    };
}

function sortPreferences(preferences: LanguagePreference[]): void {
    preferences.sort((a, b) => {
        if (b.q !== a.q) return b.q - a.q;
        if (b.specificity !== a.specificity) return b.specificity - a.specificity;
        return a.order - b.order;
    });
}

export function negotiateLanguage(header: string, supported: string[]): LanguageResult {
    if (supported.length === 0) {
        return { selected: null, reason: "no-supported", matchedRange: null, quality: 0 };
    }

    const normalizedSupported = supported.map(tag => tag.toLowerCase());
    if (header.trim().length === 0) {
        return {
            selected: supported[0],
            reason: "default",
            matchedRange: null,
            quality: 1,
        };
    }

    const preferences = header
        .split(",")
        .map((part, index) => parsePreference(part, index))
        .filter((value): value is LanguagePreference => value !== null);

    if (preferences.length === 0) {
        return {
            selected: supported[0],
            reason: "default",
            matchedRange: null,
            quality: 1,
        };
    }

    sortPreferences(preferences);

    for (const pref of preferences) {
        if (pref.q === 0) {
            continue;
        }

        if (pref.tag === "*") {
            return {
                selected: supported[0],
                reason: "wildcard",
                matchedRange: pref.tag,
                quality: pref.q,
            };
        }

        for (let i = 0; i < normalizedSupported.length; i++) {
            const candidate = normalizedSupported[i];
            if (candidate === pref.tag) {
                return {
                    selected: supported[i],
                    reason: "exact",
                    matchedRange: pref.tag,
                    quality: pref.q,
                };
            }
        }

        for (let i = 0; i < normalizedSupported.length; i++) {
            const candidate = normalizedSupported[i];
            if (candidate.slice(0, pref.primary.length + 1) === `${pref.primary}-`) {
                return {
                    selected: supported[i],
                    reason: "prefix",
                    matchedRange: pref.tag,
                    quality: pref.q,
                };
            }
        }

        if (pref.region.length > 0) {
            for (let i = 0; i < normalizedSupported.length; i++) {
                const candidate = normalizedSupported[i];
                if (candidate === pref.primary) {
                    return {
                        selected: supported[i],
                        reason: "fallback",
                        matchedRange: pref.tag,
                        quality: pref.q,
                    };
                }
            }
        }
    }

    return { selected: null, reason: "no-match", matchedRange: null, quality: 0 };
}
