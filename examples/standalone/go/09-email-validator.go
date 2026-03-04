// Example 9: Email address validator
// Validates email addresses against a subset of RFC 5321/5322 rules.
// Exercises character-class checks, length limits, and structural validation.
//
// EXPECTED BRANCHES for ValidateEmail (20):
//   1. empty string                             → invalid: "empty"
//   2. no '@' character                         → invalid: "missing @"
//   3. multiple '@' characters                  → invalid: "multiple @"
//   4. local part empty                         → invalid: "empty local part"
//   5. local part > 64 chars                    → invalid: "local part too long"
//   6. domain empty                             → invalid: "empty domain"
//   7. domain > 253 chars                       → invalid: "domain too long"
//   8. local starts with dot                    → invalid: "local starts with dot"
//   9. local ends with dot                      → invalid: "local ends with dot"
//  10. local has consecutive dots               → invalid: "consecutive dots"
//  11. local has invalid character              → invalid: "invalid character in local"
//  12. domain has no dot (no TLD)               → invalid: "domain missing TLD"
//  13. domain label starts with hyphen          → invalid: "domain label starts with hyphen"
//  14. domain label ends with hyphen            → invalid: "domain label ends with hyphen"
//  15. domain label empty (consecutive dots)    → invalid: "empty domain label"
//  16. domain label > 63 chars                  → invalid: "domain label too long"
//  17. domain has invalid character             → invalid: "invalid character in domain"
//  18. plus-addressing detected                 → valid with tag
//  19. quoted local part (starts+ends with ")   → valid, quoted
//  20. standard valid address                   → valid

package main

import "strings"

const (
	localPartMax   = 64
	domainMax      = 253
	domainLabelMax = 63
)

// EmailResult holds the validation result for an email address.
type EmailResult struct {
	Valid  bool
	Reason string
	Tag    string
	Quoted bool
}

func isValidLocalChar(c byte) bool {
	if c >= 'a' && c <= 'z' {
		return true
	}
	if c >= 'A' && c <= 'Z' {
		return true
	}
	if c >= '0' && c <= '9' {
		return true
	}
	switch c {
	case '.', '!', '#', '$', '%', '&', '\'', '*', '+', '/', '=', '?', '^', '_', '`', '{', '|', '}', '~', '-':
		return true
	}
	return false
}

func isValidDomainChar(c byte) bool {
	if c >= 'a' && c <= 'z' {
		return true
	}
	if c >= 'A' && c <= 'Z' {
		return true
	}
	if c >= '0' && c <= '9' {
		return true
	}
	return c == '-'
}

// ValidateEmail validates an email address against RFC 5321/5322 subset rules.
func ValidateEmail(email string) EmailResult {
	if len(email) == 0 {
		return EmailResult{Reason: "empty"}
	}

	atIdx := strings.Index(email, "@")
	if atIdx == -1 {
		return EmailResult{Reason: "missing @"}
	}
	if strings.Index(email[atIdx+1:], "@") != -1 {
		return EmailResult{Reason: "multiple @"}
	}

	local := email[:atIdx]
	domain := email[atIdx+1:]

	if len(local) == 0 {
		return EmailResult{Reason: "empty local part"}
	}
	if len(local) > localPartMax {
		return EmailResult{Reason: "local part too long"}
	}
	if len(domain) == 0 {
		return EmailResult{Reason: "empty domain"}
	}
	if len(domain) > domainMax {
		return EmailResult{Reason: "domain too long"}
	}

	// Quoted local part
	if len(local) >= 2 && local[0] == '"' && local[len(local)-1] == '"' {
		return EmailResult{Valid: true, Quoted: true}
	}

	// Unquoted local part validation
	if local[0] == '.' {
		return EmailResult{Reason: "local starts with dot"}
	}
	if local[len(local)-1] == '.' {
		return EmailResult{Reason: "local ends with dot"}
	}
	if strings.Contains(local, "..") {
		return EmailResult{Reason: "consecutive dots"}
	}

	for i := 0; i < len(local); i++ {
		if !isValidLocalChar(local[i]) {
			return EmailResult{Reason: "invalid character in local"}
		}
	}

	// Domain validation
	labels := strings.Split(domain, ".")
	if len(labels) < 2 {
		return EmailResult{Reason: "domain missing TLD"}
	}

	for _, label := range labels {
		if len(label) == 0 {
			return EmailResult{Reason: "empty domain label"}
		}
		if len(label) > domainLabelMax {
			return EmailResult{Reason: "domain label too long"}
		}
		if label[0] == '-' {
			return EmailResult{Reason: "domain label starts with hyphen"}
		}
		if label[len(label)-1] == '-' {
			return EmailResult{Reason: "domain label ends with hyphen"}
		}
		for i := 0; i < len(label); i++ {
			if !isValidDomainChar(label[i]) {
				return EmailResult{Reason: "invalid character in domain"}
			}
		}
	}

	// Plus-addressing
	if plusIdx := strings.Index(local, "+"); plusIdx != -1 {
		return EmailResult{Valid: true, Tag: local[plusIdx+1:]}
	}

	return EmailResult{Valid: true}
}
