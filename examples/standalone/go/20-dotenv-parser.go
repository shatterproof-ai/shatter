// Example 20: dotenv parser
// Adapted from motdotla/dotenv (BSD-2-Clause): https://github.com/motdotla/dotenv
// Line parsing, export-prefix handling, quoted values, comments, and multiline quotes.
//
// EXPECTED BRANCHES for ParseDotenv (16):
//   1. empty input                               -> empty result
//   2. blank lines ignored                       -> skipped
//   3. comment lines ignored                     -> skipped
//   4. invalid line without separator            -> warning emitted
//   5. invalid key characters                    -> warning emitted
//   6. export prefix stripped                    -> key parsed
//   7. '=' separator parsed                      -> value assigned
//   8. ':' separator parsed                      -> value assigned
//   9. empty value allowed                       -> empty string
//  10. unquoted values trimmed                   -> whitespace removed
//  11. inline comment on unquoted value removed  -> comment stripped
//  12. single-quoted value preserved             -> escapes left literal
//  13. double-quoted value expands \n and \r     -> escaped newlines expanded
//  14. hash inside quoted value preserved        -> not treated as comment
//  15. multiline quoted value consumed           -> joins following lines
//  16. unterminated quoted value                 -> warning emitted
//
// DIFFICULTY: Hard. The solver must craft line-oriented text with quotes,
// separators, comments, and multiline structure that interact precisely.

package main

import (
	"fmt"
	"strings"
)

type DotenvResult struct {
	Values   map[string]string
	Warnings []string
}

func findDotenvSeparator(line string) int {
	eq := strings.Index(line, "=")
	colon := strings.Index(line, ":")
	if eq == -1 {
		return colon
	}
	if colon == -1 {
		return eq
	}
	if eq < colon {
		return eq
	}
	return colon
}

func stripInlineDotenvComment(value string) string {
	inSingle := false
	inDouble := false
	inBacktick := false
	for i, ch := range value {
		switch ch {
		case '\'':
			if !inDouble && !inBacktick {
				inSingle = !inSingle
			}
		case '"':
			if !inSingle && !inBacktick {
				inDouble = !inDouble
			}
		case '`':
			if !inSingle && !inDouble {
				inBacktick = !inBacktick
			}
		case '#':
			if !inSingle && !inDouble && !inBacktick {
				return value[:i]
			}
		}
	}
	return value
}

func validDotenvKey(key string) bool {
	if key == "" {
		return false
	}
	for _, ch := range key {
		if !(ch >= 'A' && ch <= 'Z' || ch >= 'a' && ch <= 'z' || ch >= '0' && ch <= '9' || ch == '_' || ch == '.' || ch == '-') {
			return false
		}
	}
	return true
}

// ParseDotenv parses a dotenv-like source string into a map plus warnings.
func ParseDotenv(src string) DotenvResult {
	values := map[string]string{}
	warnings := make([]string, 0)
	lines := strings.Split(strings.ReplaceAll(strings.ReplaceAll(src, "\r\n", "\n"), "\r", "\n"), "\n")

	for i := 0; i < len(lines); i++ {
		line := strings.TrimSpace(lines[i])
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		if strings.HasPrefix(line, "export ") {
			line = strings.TrimSpace(line[len("export "):])
		}

		sep := findDotenvSeparator(line)
		if sep < 0 {
			warnings = append(warnings, fmt.Sprintf("line %d: missing separator", i+1))
			continue
		}

		key := strings.TrimSpace(line[:sep])
		if !validDotenvKey(key) {
			warnings = append(warnings, fmt.Sprintf("line %d: invalid key", i+1))
			continue
		}

		rawValue := strings.TrimLeft(line[sep+1:], " \t")
		if rawValue == "" {
			values[key] = ""
			continue
		}

		quote := rawValue[0]
		if quote == '\'' || quote == '"' || quote == '`' {
			body := rawValue[1:]
			closed := false

			for {
				if idx := strings.IndexByte(body, quote); idx >= 0 {
					body = body[:idx]
					closed = true
					break
				}
				if i+1 >= len(lines) {
					break
				}
				i++
				body += "\n" + lines[i]
			}

			if !closed {
				warnings = append(warnings, fmt.Sprintf("line %d: unterminated quote", i+1))
				continue
			}

			if quote == '"' {
				body = strings.ReplaceAll(body, `\n`, "\n")
				body = strings.ReplaceAll(body, `\r`, "\r")
			}

			values[key] = body
			continue
		}

		values[key] = strings.TrimSpace(stripInlineDotenvComment(rawValue))
	}

	return DotenvResult{Values: values, Warnings: warnings}
}

func main() {
	result := ParseDotenv("export NAME=shatter\nEMPTY=\nQUOTE=\"hello\\nworld\"")
	fmt.Printf("values=%v warnings=%v\n", result.Values, result.Warnings)
}
