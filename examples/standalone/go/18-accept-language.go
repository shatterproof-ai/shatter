// Example 18: Accept-Language negotiation
// Adapted from jshttp/negotiator (MIT): https://github.com/jshttp/negotiator
// Header parsing, q-value ranking, wildcard handling, and exact-versus-prefix matching.
//
// EXPECTED BRANCHES for NegotiateLanguage (13):
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

package main

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
)

type LanguagePreference struct {
	Tag         string
	Primary     string
	Region      string
	Q           float64
	Specificity int
	Order       int
}

type LanguageResult struct {
	Selected     *string
	Reason       string
	MatchedRange *string
	Quality      float64
}

func parseLanguagePreference(part string, order int) (*LanguagePreference, bool) {
	trimmed := strings.TrimSpace(part)
	if trimmed == "" {
		return nil, false
	}

	segments := strings.Split(trimmed, ";")
	tag := strings.TrimSpace(segments[0])
	normalized := strings.ToLower(tag)
	if normalized != "*" {
		pieces := strings.Split(normalized, "-")
		if len(pieces) == 0 || len(pieces) > 2 {
			return nil, false
		}
		for _, piece := range pieces {
			if piece == "" {
				return nil, false
			}
			for _, ch := range piece {
				if !(ch >= 'a' && ch <= 'z' || ch >= '0' && ch <= '9') {
					return nil, false
				}
			}
		}
	}

	q := 1.0
	for _, segment := range segments[1:] {
		parts := strings.SplitN(segment, "=", 2)
		if len(parts) != 2 {
			continue
		}
		if strings.TrimSpace(parts[0]) != "q" {
			continue
		}
		parsed, err := strconv.ParseFloat(strings.TrimSpace(parts[1]), 64)
		if err != nil || parsed < 0 || parsed > 1 {
			return nil, false
		}
		q = parsed
	}

	if normalized == "*" {
		return &LanguagePreference{
			Tag:         normalized,
			Primary:     "*",
			Q:           q,
			Specificity: 0,
			Order:       order,
		}, true
	}

	pieces := strings.SplitN(normalized, "-", 2)
	region := ""
	specificity := 1
	if len(pieces) == 2 {
		region = pieces[1]
		specificity = 2
	}

	return &LanguagePreference{
		Tag:         normalized,
		Primary:     pieces[0],
		Region:      region,
		Q:           q,
		Specificity: specificity,
		Order:       order,
	}, true
}

// NegotiateLanguage chooses the best supported language for an Accept-Language header.
func NegotiateLanguage(header string, supported []string) LanguageResult {
	if len(supported) == 0 {
		return LanguageResult{Reason: "no-supported"}
	}

	if strings.TrimSpace(header) == "" {
		selected := supported[0]
		return LanguageResult{Selected: &selected, Reason: "default", Quality: 1}
	}

	preferences := make([]LanguagePreference, 0)
	for i, part := range strings.Split(header, ",") {
		pref, ok := parseLanguagePreference(part, i)
		if ok {
			preferences = append(preferences, *pref)
		}
	}

	if len(preferences) == 0 {
		selected := supported[0]
		return LanguageResult{Selected: &selected, Reason: "default", Quality: 1}
	}

	sort.Slice(preferences, func(i, j int) bool {
		if preferences[i].Q != preferences[j].Q {
			return preferences[i].Q > preferences[j].Q
		}
		if preferences[i].Specificity != preferences[j].Specificity {
			return preferences[i].Specificity > preferences[j].Specificity
		}
		return preferences[i].Order < preferences[j].Order
	})

	normalizedSupported := make([]string, len(supported))
	for i, tag := range supported {
		normalizedSupported[i] = strings.ToLower(tag)
	}

	for _, pref := range preferences {
		if pref.Q == 0 {
			continue
		}

		if pref.Tag == "*" {
			selected := supported[0]
			matched := pref.Tag
			return LanguageResult{
				Selected:     &selected,
				Reason:       "wildcard",
				MatchedRange: &matched,
				Quality:      pref.Q,
			}
		}

		for i, candidate := range normalizedSupported {
			if candidate == pref.Tag {
				selected := supported[i]
				matched := pref.Tag
				return LanguageResult{
					Selected:     &selected,
					Reason:       "exact",
					MatchedRange: &matched,
					Quality:      pref.Q,
				}
			}
		}

		for i, candidate := range normalizedSupported {
			if strings.HasPrefix(candidate, pref.Primary+"-") {
				selected := supported[i]
				matched := pref.Tag
				return LanguageResult{
					Selected:     &selected,
					Reason:       "prefix",
					MatchedRange: &matched,
					Quality:      pref.Q,
				}
			}
		}

		if pref.Region != "" {
			for i, candidate := range normalizedSupported {
				if candidate == pref.Primary {
					selected := supported[i]
					matched := pref.Tag
					return LanguageResult{
						Selected:     &selected,
						Reason:       "fallback",
						MatchedRange: &matched,
						Quality:      pref.Q,
					}
				}
			}
		}
	}

	return LanguageResult{Reason: "no-match"}
}

func main() {
	result := NegotiateLanguage("fr-CA, fr;q=0.9, en;q=0.5", []string{"en", "fr-FR", "fr"})
	fmt.Printf("selected=%v reason=%s quality=%.1f\n", result.Selected, result.Reason, result.Quality)
}
