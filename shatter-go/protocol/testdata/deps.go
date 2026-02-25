package testdata

import (
	"fmt"
	"strings"
)

// FormatName uses external functions from fmt and strings packages.
func FormatName(first, last string) string {
	full := strings.TrimSpace(first) + " " + strings.TrimSpace(last)
	return fmt.Sprintf("Name: %s", full)
}
