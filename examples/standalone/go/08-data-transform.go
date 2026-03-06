package main

import (
	"fmt"
	"math"
	"regexp"
	"strconv"
	"strings"
)

// Example 8: Data transformation and config merging.
// Tests reasoning about map types, recursive depth, and multi-step validation.

// MergeConfig — 10 branches: both empty→{}, override-only key→added,
// base-only key→preserved, both objects→recursive, both slices+append→concat,
// both slices+replace→override, override nil→removed, type mismatch→override,
// same type→override, depth exceeded→error.
func MergeConfig(
	base, override map[string]any,
	arrayStrategy string,
	maxDepth int,
	currentDepth int,
) (map[string]any, error) {
	if currentDepth > maxDepth {
		return nil, fmt.Errorf("max depth exceeded")
	}

	result := make(map[string]any)

	for key, baseVal := range base {
		overrideVal, exists := override[key]
		if !exists {
			result[key] = baseVal
			continue
		}

		if overrideVal == nil {
			continue
		}

		baseMap, baseIsMap := baseVal.(map[string]any)
		overrideMap, overrideIsMap := overrideVal.(map[string]any)
		if baseIsMap && overrideIsMap {
			merged, err := MergeConfig(baseMap, overrideMap, arrayStrategy, maxDepth, currentDepth+1)
			if err != nil {
				return nil, err
			}
			result[key] = merged
			continue
		}

		baseSlice, baseIsSlice := baseVal.([]any)
		overrideSlice, overrideIsSlice := overrideVal.([]any)
		if baseIsSlice && overrideIsSlice {
			if arrayStrategy == "append" {
				combined := make([]any, 0, len(baseSlice)+len(overrideSlice))
				combined = append(combined, baseSlice...)
				combined = append(combined, overrideSlice...)
				result[key] = combined
			} else {
				result[key] = overrideSlice
			}
			continue
		}

		result[key] = overrideVal
	}

	for key, overrideVal := range override {
		if _, exists := base[key]; !exists && overrideVal != nil {
			result[key] = overrideVal
		}
	}

	return result, nil
}

// TransformResult holds the outcome of a record transformation.
type TransformResult struct {
	Status     string
	Reason     string
	Normalized map[string]any
}

var simpleEmailRegex = regexp.MustCompile(`^[^@]+@[^@]+\.[^@]+$`)

// TransformRecord — 9 branches: missing id→rejected, missing type→rejected,
// user+no email→rejected, user+invalid email→rejected, user+valid→accepted,
// order+no amount→rejected, order+amount≤0→rejected, order+valid→accepted,
// unknown type→rejected.
func TransformRecord(record map[string]any) TransformResult {
	id, hasID := record["id"]
	if !hasID || id == nil || id == "" {
		return TransformResult{Status: "rejected", Reason: "missing id"}
	}

	typ, hasType := record["type"]
	if !hasType || typ == nil || typ == "" {
		return TransformResult{Status: "rejected", Reason: "missing type"}
	}

	typeStr := fmt.Sprintf("%v", typ)

	if typeStr == "user" {
		email, hasEmail := record["email"]
		if !hasEmail || email == nil || email == "" {
			return TransformResult{Status: "rejected", Reason: "user needs email"}
		}
		emailStr := fmt.Sprintf("%v", email)
		if !simpleEmailRegex.MatchString(emailStr) {
			return TransformResult{Status: "rejected", Reason: "invalid email"}
		}
		return TransformResult{
			Status: "accepted",
			Normalized: map[string]any{
				"id":    fmt.Sprintf("%v", id),
				"type":  "user",
				"email": strings.ToLower(emailStr),
			},
		}
	}

	if typeStr == "order" {
		amount, hasAmount := record["amount"]
		if !hasAmount || amount == nil || amount == "" {
			return TransformResult{Status: "rejected", Reason: "order needs amount"}
		}
		numAmount, err := strconv.ParseFloat(fmt.Sprintf("%v", amount), 64)
		if err != nil || numAmount <= 0 {
			return TransformResult{Status: "rejected", Reason: "non-positive amount"}
		}
		return TransformResult{
			Status: "accepted",
			Normalized: map[string]any{
				"id":     fmt.Sprintf("%v", id),
				"type":   "order",
				"amount": math.Round(numAmount*100) / 100,
			},
		}
	}

	return TransformResult{Status: "rejected", Reason: "unknown type"}
}
