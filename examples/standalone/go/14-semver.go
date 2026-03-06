// Example 8: Semantic versioning parser and range matcher
// Parses semver strings and checks if versions satisfy range constraints.
// Exercises numeric parsing, string splitting, multi-field comparison, and
// operator dispatch.
//
// EXPECTED BRANCHES for ParseSemver (12):
//   1. empty string                             → error: "invalid semver"
//   2. missing minor version (e.g. "1")         → error: "invalid semver"
//   3. missing patch version (e.g. "1.2")       → error: "invalid semver"
//   4. non-numeric major (e.g. "a.0.0")         → error: "invalid semver"
//   5. non-numeric minor                        → error: "invalid semver"
//   6. non-numeric patch base                   → error: "invalid semver"
//   7. negative component                       → error: "invalid semver"
//   8. valid x.y.z no prerelease                → Semver{major, minor, patch, ""}
//   9. valid with prerelease (e.g. "1.0.0-alpha") → Semver{..., "alpha"}
//  10. patch contains hyphen for prerelease     → splits on first hyphen
//  11. leading 'v' prefix stripped               → "v1.2.3" parsed as "1.2.3"
//  12. extra parts after patch ignored           → only first 3 parts used
//
// EXPECTED BRANCHES for SatisfiesRange (19):
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
//  19. unknown operator                         → error: "unknown range operator"

package main

import (
	"errors"
	"strconv"
	"strings"
)

// Semver represents a parsed semantic version.
type Semver struct {
	Major      int
	Minor      int
	Patch      int
	Prerelease string
}

// ParseSemver parses a semantic version string into its components.
func ParseSemver(version string) (Semver, error) {
	if len(version) == 0 {
		return Semver{}, errors.New("invalid semver")
	}

	v := version
	if v[0] == 'v' || v[0] == 'V' {
		v = v[1:]
	}

	parts := strings.SplitN(v, ".", 3)
	if len(parts) < 3 {
		return Semver{}, errors.New("invalid semver")
	}

	major, err := strconv.Atoi(parts[0])
	if err != nil {
		return Semver{}, errors.New("invalid semver")
	}

	minor, err := strconv.Atoi(parts[1])
	if err != nil {
		return Semver{}, errors.New("invalid semver")
	}

	patchStr := parts[2]
	prerelease := ""
	if idx := strings.Index(patchStr, "-"); idx >= 0 {
		prerelease = patchStr[idx+1:]
		patchStr = patchStr[:idx]
	}

	patch, err := strconv.Atoi(patchStr)
	if err != nil {
		return Semver{}, errors.New("invalid semver")
	}

	if major < 0 || minor < 0 || patch < 0 {
		return Semver{}, errors.New("invalid semver")
	}

	return Semver{Major: major, Minor: minor, Patch: patch, Prerelease: prerelease}, nil
}

func compareSemver(a, b Semver) int {
	if a.Major != b.Major {
		return a.Major - b.Major
	}
	if a.Minor != b.Minor {
		return a.Minor - b.Minor
	}
	if a.Patch != b.Patch {
		return a.Patch - b.Patch
	}

	if a.Prerelease == "" && b.Prerelease == "" {
		return 0
	}
	if a.Prerelease == "" {
		return 1
	}
	if b.Prerelease == "" {
		return -1
	}
	if a.Prerelease < b.Prerelease {
		return -1
	}
	if a.Prerelease > b.Prerelease {
		return 1
	}
	return 0
}

// SatisfiesRange checks if a version string satisfies a semver range constraint.
// Supports operators: >= > <= < ^ ~ and exact match. Wildcards: * x.
func SatisfiesRange(version, rangeExpr string) (bool, error) {
	if rangeExpr == "*" || rangeExpr == "x" {
		return true, nil
	}

	operator := ""
	rangeVersion := rangeExpr

	if strings.HasPrefix(rangeExpr, ">=") {
		operator = ">="
		rangeVersion = rangeExpr[2:]
	} else if strings.HasPrefix(rangeExpr, ">") {
		operator = ">"
		rangeVersion = rangeExpr[1:]
	} else if strings.HasPrefix(rangeExpr, "<=") {
		operator = "<="
		rangeVersion = rangeExpr[2:]
	} else if strings.HasPrefix(rangeExpr, "<") {
		operator = "<"
		rangeVersion = rangeExpr[1:]
	} else if strings.HasPrefix(rangeExpr, "^") {
		operator = "^"
		rangeVersion = rangeExpr[1:]
	} else if strings.HasPrefix(rangeExpr, "~") {
		operator = "~"
		rangeVersion = rangeExpr[1:]
	}

	ver, err := ParseSemver(version)
	if err != nil {
		return false, err
	}
	target, err := ParseSemver(rangeVersion)
	if err != nil {
		return false, err
	}

	cmp := compareSemver(ver, target)

	switch operator {
	case "":
		return cmp == 0, nil
	case ">=":
		return cmp >= 0, nil
	case ">":
		return cmp > 0, nil
	case "<=":
		return cmp <= 0, nil
	case "<":
		return cmp < 0, nil
	case "^":
		if ver.Major != target.Major {
			return false, nil
		}
		return cmp >= 0, nil
	case "~":
		if ver.Major != target.Major || ver.Minor != target.Minor {
			return false, nil
		}
		return cmp >= 0, nil
	default:
		return false, errors.New("unknown range operator")
	}
}
