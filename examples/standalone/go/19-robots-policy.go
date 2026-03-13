// Example 19: robots.txt rule evaluation
// Adapted from samclarke/robots-parser (MIT): https://github.com/samclarke/robots-parser
// Directive parsing, user-agent grouping, wildcard path matching, and precedence.
//
// EXPECTED BRANCHES for EvaluateRobotsPolicy (15):
//   1. empty user agent                          -> missing-user-agent
//   2. path missing leading slash                -> invalid-path
//   3. comments and blank lines ignored          -> parse continues
//   4. unknown directives ignored                -> parse continues
//   5. specific user-agent group beats wildcard  -> specific group selected
//   6. wildcard user-agent fallback              -> wildcard group selected
//   7. no matching group                         -> allow by default
//   8. empty disallow directive                  -> ignored
//   9. valid crawl-delay parsed                  -> CrawlDelay populated
//  10. invalid crawl-delay ignored               -> CrawlDelay absent
//  11. wildcard '*' in rule matches segment      -> rule applies
//  12. '$' end-anchor enforced                   -> suffix-sensitive match
//  13. longer allow beats shorter disallow       -> allowed
//  14. longer disallow beats allow               -> disallowed
//  15. equal-length tie resolved in favor allow  -> allowed
//
// DIFFICULTY: Hard. The solver must synthesize structured multi-line rule sets
// with interacting directives and carefully chosen paths.

package main

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

type RobotsRule struct {
	Directive string
	Pattern   string
}

type RobotsGroup struct {
	Agents     []string
	Rules      []RobotsRule
	CrawlDelay *float64
}

type RobotsDecision struct {
	Allowed          bool
	Reason           string
	MatchedDirective *string
	CrawlDelay       *float64
}

func normalizeRobotAgent(agent string) string {
	lower := strings.ToLower(strings.TrimSpace(agent))
	if idx := strings.Index(lower, "/"); idx >= 0 {
		return lower[:idx]
	}
	return lower
}

func parseRobotsGroups(robotsTxt string) []RobotsGroup {
	lines := strings.Split(strings.ReplaceAll(strings.ReplaceAll(robotsTxt, "\r\n", "\n"), "\r", "\n"), "\n")
	groups := make([]RobotsGroup, 0)
	var current *RobotsGroup

	for _, rawLine := range lines {
		line := rawLine
		if idx := strings.Index(line, "#"); idx >= 0 {
			line = line[:idx]
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		colon := strings.Index(line, ":")
		if colon < 0 {
			continue
		}

		key := strings.ToLower(strings.TrimSpace(line[:colon]))
		value := strings.TrimSpace(line[colon+1:])

		if key == "user-agent" {
			if current == nil || len(current.Rules) > 0 || current.CrawlDelay != nil {
				groups = append(groups, RobotsGroup{})
				current = &groups[len(groups)-1]
			}
			current.Agents = append(current.Agents, normalizeRobotAgent(value))
			continue
		}

		if current == nil {
			continue
		}

		switch key {
		case "allow", "disallow":
			if key == "disallow" && value == "" {
				continue
			}
			current.Rules = append(current.Rules, RobotsRule{Directive: key, Pattern: value})
		case "crawl-delay":
			delay, err := strconv.ParseFloat(value, 64)
			if err == nil && delay >= 0 {
				current.CrawlDelay = &delay
			}
		}
	}

	return groups
}

func groupMatchesRobotAgent(agent string, group RobotsGroup) bool {
	for _, candidate := range group.Agents {
		if candidate == "*" {
			continue
		}
		if strings.HasPrefix(agent, candidate) {
			return true
		}
	}
	return false
}

func matchesRobotPattern(pattern string, path string) bool {
	anchored := strings.HasSuffix(pattern, "$")
	body := pattern
	if anchored {
		body = pattern[:len(pattern)-1]
	}
	replacer := strings.NewReplacer(
		"\\", "\\\\",
		".", "\\.",
		"+", "\\+",
		"?", "\\?",
		"(", "\\(",
		")", "\\)",
		"[", "\\[",
		"]", "\\]",
		"{", "\\{",
		"}", "\\}",
		"|", "\\|",
		"^", "\\^",
		"$", "\\$",
	)
	source := "^" + strings.ReplaceAll(replacer.Replace(body), "*", ".*")
	if anchored {
		source += "$"
	}
	matched, err := regexp.MatchString(source, path)
	return err == nil && matched
}

func robotRuleSpecificity(pattern string) int {
	pattern = strings.ReplaceAll(pattern, "*", "")
	pattern = strings.ReplaceAll(pattern, "$", "")
	return len(pattern)
}

// EvaluateRobotsPolicy chooses the effective rule for a user agent and path.
func EvaluateRobotsPolicy(robotsTxt string, userAgent string, path string) RobotsDecision {
	normalizedAgent := normalizeRobotAgent(userAgent)
	if normalizedAgent == "" {
		return RobotsDecision{Reason: "missing-user-agent"}
	}
	if !strings.HasPrefix(path, "/") {
		return RobotsDecision{Reason: "invalid-path"}
	}

	groups := parseRobotsGroups(robotsTxt)
	specificGroups := make([]RobotsGroup, 0)
	wildcardGroups := make([]RobotsGroup, 0)
	for _, group := range groups {
		if groupMatchesRobotAgent(normalizedAgent, group) {
			specificGroups = append(specificGroups, group)
		}
		for _, candidate := range group.Agents {
			if candidate == "*" {
				wildcardGroups = append(wildcardGroups, group)
				break
			}
		}
	}

	selectedGroups := specificGroups
	if len(selectedGroups) == 0 {
		selectedGroups = wildcardGroups
	}
	if len(selectedGroups) == 0 {
		return RobotsDecision{Allowed: true, Reason: "no-group"}
	}

	var crawlDelay *float64
	var bestRule *RobotsRule
	bestSpecificity := -1

	for _, group := range selectedGroups {
		if crawlDelay == nil && group.CrawlDelay != nil {
			crawlDelay = group.CrawlDelay
		}
		for _, rule := range group.Rules {
			if !matchesRobotPattern(rule.Pattern, path) {
				continue
			}
			specificity := robotRuleSpecificity(rule.Pattern)
			betterDirective := bestRule != nil &&
				specificity == bestSpecificity &&
				rule.Directive == "allow" &&
				bestRule.Directive == "disallow"
			if bestRule == nil || specificity > bestSpecificity || betterDirective {
				copyRule := rule
				bestRule = &copyRule
				bestSpecificity = specificity
			}
		}
	}

	if bestRule == nil {
		return RobotsDecision{Allowed: true, Reason: "no-rule", CrawlDelay: crawlDelay}
	}

	matched := bestRule.Directive + ":" + bestRule.Pattern
	return RobotsDecision{
		Allowed:          bestRule.Directive == "allow",
		Reason:           bestRule.Directive,
		MatchedDirective: &matched,
		CrawlDelay:       crawlDelay,
	}
}

func main() {
	robots := "User-agent: *\nDisallow: /private/\nAllow: /private/public$\nCrawl-delay: 2"
	result := EvaluateRobotsPolicy(robots, "DocsBot/1.0", "/private/public")
	fmt.Printf("allowed=%v reason=%s matched=%v\n", result.Allowed, result.Reason, result.MatchedDirective)
}
