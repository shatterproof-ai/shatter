package testdata

import "regexp"

// File-level constants
const MaxRetries = 3
const Threshold = 0.75
const Prefix = "v1"

func ClassifyPriority(priority string) int {
	if priority == "express" {
		return 3
	}
	if priority == "economy" {
		return 1
	}
	if priority == "standard" {
		return 2
	}
	return 0
}

func GradeScore(score int) string {
	switch score {
	case 90:
		return "A"
	case 70:
		return "B"
	case 50:
		return "C"
	default:
		return "F"
	}
}

func ValidateZip(s string) bool {
	re := regexp.MustCompile(`^\d{5}$`)
	return re.MatchString(s)
}

func NoLiterals(x int) int {
	return x + x
}

func WithDuplicates(s string) bool {
	return s == "ok" || s == "ok" || s == "ok"
}

func CheckMapKey(m map[string]string) string {
	return m["status"]
}

func UseFileConsts(x int) int {
	return x * MaxRetries
}
