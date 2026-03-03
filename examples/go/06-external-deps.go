// Example 6: External dependencies and auto-mocking
// Tests shatter's automatic mock generation for functions that depend on
// external standard library packages: os (filesystem), fmt (utility),
// strings (pure utility), and strconv (pure utility).
//
// Shatter classifies each dependency and generates appropriate mocks:
//   - os             → filesystem I/O stubs
//   - net/http       → network stubs
//   - database/sql   → database stubs
//   - fmt/strings    → pure utility passthrough
//
// EXPECTED BRANCHES for LoadConfigFile (4):
//   1. path is empty           -> "error: empty path"
//   2. file read fails         -> "error: read failed"
//   3. content is empty        -> "empty"
//   4. content has data        -> "loaded:<length>"
//
// EXPECTED BRANCHES for FormatUserRecord (4):
//   1. name is empty           -> "error: empty name"
//   2. age < 0                 -> "error: negative age"
//   3. age == 0                -> "<name> (age unknown)"
//   4. age > 0                 -> "<name> (age <age>)"
//
// EXPECTED BRANCHES for ParseAndValidate (5):
//   1. input is empty          -> "error: empty input"
//   2. not a valid integer     -> "error: not a number"
//   3. value < 0               -> "negative"
//   4. value == 0              -> "zero"
//   5. value > 0               -> "positive"

package examples

import (
	"fmt"
	"os"
	"strconv"
	"strings"
)

// LoadConfigFile reads a config file and returns a status string.
// Depends on os.ReadFile (filesystem I/O — should be auto-mocked).
func LoadConfigFile(path string) string {
	if strings.TrimSpace(path) == "" {
		return "error: empty path"
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return "error: read failed"
	}

	content := string(data)
	if len(content) == 0 {
		return "empty"
	}

	return fmt.Sprintf("loaded:%d", len(content))
}

// FormatUserRecord formats a user's name and age into a display string.
// Depends on fmt.Sprintf (pure utility — should be passthrough) and
// strings.TrimSpace (pure utility — should be passthrough).
func FormatUserRecord(name string, age int) string {
	name = strings.TrimSpace(name)
	if name == "" {
		return "error: empty name"
	}

	if age < 0 {
		return "error: negative age"
	}

	if age == 0 {
		return fmt.Sprintf("%s (age unknown)", name)
	}

	return fmt.Sprintf("%s (age %d)", name, age)
}

// ParseAndValidate parses a string to an integer and classifies it.
// Depends on strconv.Atoi (pure utility — should be passthrough) and
// strings.TrimSpace (pure utility — should be passthrough).
func ParseAndValidate(input string) string {
	input = strings.TrimSpace(input)
	if input == "" {
		return "error: empty input"
	}

	value, err := strconv.Atoi(input)
	if err != nil {
		return "error: not a number"
	}

	if value < 0 {
		return "negative"
	}
	if value == 0 {
		return "zero"
	}
	return "positive"
}
