// Example 14: Semantic versioning parser and range matcher
// Parses semver strings and checks if versions satisfy range constraints.
// Exercises numeric parsing, string splitting, multi-field comparison, and
// operator dispatch — a pattern common in package managers and CI systems.
//
// EXPECTED BRANCHES for parseSemver (12):
//   1. empty string                             → throws Error("invalid semver")
//   2. missing minor version (e.g. "1")         → throws Error("invalid semver")
//   3. missing patch version (e.g. "1.2")       → throws Error("invalid semver")
//   4. non-numeric major (e.g. "a.0.0")         → throws Error("invalid semver")
//   5. non-numeric minor                        → throws Error("invalid semver")
//   6. non-numeric patch base                   → throws Error("invalid semver")
//   7. negative component                       → throws Error("invalid semver")
//   8. valid x.y.z no prerelease                → { major, minor, patch, prerelease: "" }
//   9. valid with prerelease (e.g. "1.0.0-alpha") → { major, minor, patch, prerelease: "alpha" }
//  10. patch contains hyphen for prerelease     → splits on first hyphen
//  11. leading 'v' prefix stripped               → "v1.2.3" parsed as "1.2.3"
//  12. extra parts after patch ignored           → only first 3 parts used
//
// EXPECTED BRANCHES for satisfiesRange (19):
//   1. exact match "1.2.3"                      → version equals exactly
//   2. ">=" operator, version greater           → true
//   3. ">=" operator, version equal             → true
//   4. ">=" operator, version less              → false
//   5. ">" operator, version greater            → true
//   6. ">" operator, version equal              → false
//   7. "<=" operator, version less              → true
//   8. "<=" operator, version equal             → true
//   9. "<=" operator, version greater           → false
//  10. "<" operator, version less               → true
//  11. "<" operator, version equal              → false
//  12. "^" (caret), same major, minor >=        → true
//  13. "^" (caret), same major, minor <         → false (patch decides)
//  14. "^" (caret), different major             → false
//  15. "~" (tilde), same major.minor, patch >=  → true
//  16. "~" (tilde), same major.minor, patch <   → false
//  17. "~" (tilde), different minor             → false
//  18. wildcard "x" or "*" in range             → always true
//  19. unknown operator                         → throws Error("unknown range operator")
//
// DIFFICULTY: Hard. Combines string parsing, numeric comparison across three
// fields, operator dispatch, and pre-release ordering. The solver must generate
// specific version strings that exercise each comparison branch.

interface Semver {
    major: number;
    minor: number;
    patch: number;
    prerelease: string;
}

export function parseSemver(version: string): Semver {
    if (version.length === 0) {
        throw new Error("invalid semver");
    }

    let v = version;
    if (v.startsWith("v") || v.startsWith("V")) {
        v = v.slice(1);
    }

    const parts = v.split(".");
    if (parts.length < 3) {
        throw new Error("invalid semver");
    }

    const major = parseInt(parts[0], 10);
    const minor = parseInt(parts[1], 10);

    let patchStr = parts[2];
    let prerelease = "";
    const hyphenIdx = patchStr.indexOf("-");
    if (hyphenIdx >= 0) {
        prerelease = patchStr.slice(hyphenIdx + 1);
        patchStr = patchStr.slice(0, hyphenIdx);
    }

    const patch = parseInt(patchStr, 10);

    if (isNaN(major) || isNaN(minor) || isNaN(patch)) {
        throw new Error("invalid semver");
    }
    if (major < 0 || minor < 0 || patch < 0) {
        throw new Error("invalid semver");
    }

    return { major, minor, patch, prerelease };
}

function compareSemver(a: Semver, b: Semver): number {
    if (a.major !== b.major) return a.major - b.major;
    if (a.minor !== b.minor) return a.minor - b.minor;
    if (a.patch !== b.patch) return a.patch - b.patch;

    if (a.prerelease === "" && b.prerelease === "") return 0;
    if (a.prerelease === "") return 1;
    if (b.prerelease === "") return -1;
    return a.prerelease < b.prerelease ? -1 : a.prerelease > b.prerelease ? 1 : 0;
}

export function satisfiesRange(version: string, range: string): boolean {
    if (range === "*" || range === "x") {
        return true;
    }

    let operator = "";
    let rangeVersion = range;

    if (range.startsWith(">=")) {
        operator = ">=";
        rangeVersion = range.slice(2);
    } else if (range.startsWith(">")) {
        operator = ">";
        rangeVersion = range.slice(1);
    } else if (range.startsWith("<=")) {
        operator = "<=";
        rangeVersion = range.slice(2);
    } else if (range.startsWith("<")) {
        operator = "<";
        rangeVersion = range.slice(1);
    } else if (range.startsWith("^")) {
        operator = "^";
        rangeVersion = range.slice(1);
    } else if (range.startsWith("~")) {
        operator = "~";
        rangeVersion = range.slice(1);
    }

    const ver = parseSemver(version);
    const target = parseSemver(rangeVersion);
    const cmp = compareSemver(ver, target);

    switch (operator) {
        case "":
            return cmp === 0;
        case ">=":
            return cmp >= 0;
        case ">":
            return cmp > 0;
        case "<=":
            return cmp <= 0;
        case "<":
            return cmp < 0;
        case "^":
            if (ver.major !== target.major) return false;
            return cmp >= 0;
        case "~":
            if (ver.major !== target.major || ver.minor !== target.minor) return false;
            return cmp >= 0;
        default:
            throw new Error("unknown range operator");
    }
}
