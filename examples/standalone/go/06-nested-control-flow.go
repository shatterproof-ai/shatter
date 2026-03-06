// Example 4: Complex nested control flow
// Tests shatter's ability to explore deeply nested conditionals and state machines.
//
// EXPECTED BRANCHES for ClassifyHttpResponse (15):
//   1. status < 100                              -> error: "invalid status"
//   2. status >= 600                             -> error: "invalid status"
//   3. status 100-199                            -> "informational"
//   4. status 200, contentType starts "text/"    -> "ok-text"
//   5. status 200, contentType starts "application/json" -> "ok-json"
//   6. status 200, other contentType             -> "ok-binary"
//   7. status 201, body non-empty                -> "created-with-body"
//   8. status 201, body empty                    -> "created-empty"
//   9. status 204                                -> "no-content"
//  10. status 202-203 or 205-299                 -> "success-other"
//  11. status 301 or 302                         -> "redirect"
//  12. status 300 or 303-399                     -> "redirect-other"
//  13. status 400-499, 401 or 403                -> "auth-error"
//  14. status 400-499, other                     -> "client-error"
//  15. status 500-599                            -> "server-error"
//
// EXPECTED BRANCHES for ProcessStateMachine (12):
//   1. empty events                              -> "idle"
//   2. idle + "start"                            -> transitions to loading
//   3. idle + non-"start"                        -> "invalid-transition"
//   4. loading + "success"                       -> transitions to success
//   5. loading + "error", retries left           -> stays in loading
//   6. loading + "error", no retries             -> transitions to error
//   7. loading + other                           -> "invalid-transition"
//   8. success + "reset"                         -> transitions to done
//   9. success + other                           -> "invalid-transition"
//  10. error + "reset"                           -> transitions to done
//  11. error + other                             -> "invalid-transition"
//  12. done state reached                        -> "done"

package main

import (
	"errors"
	"strings"
)

// ClassifyHttpResponse categorizes an HTTP response by its status code,
// content type, and body presence. The deep nesting across numeric ranges
// and string prefixes makes this hard for random input generation.
func ClassifyHttpResponse(status int, contentType string, body string) (string, error) {
	if status < 100 || status >= 600 {
		return "", errors.New("invalid status")
	}

	if status < 200 {
		return "informational", nil
	}

	if status < 300 {
		if status == 200 {
			if strings.HasPrefix(contentType, "text/") {
				return "ok-text", nil
			}
			if strings.HasPrefix(contentType, "application/json") {
				return "ok-json", nil
			}
			return "ok-binary", nil
		}
		if status == 201 {
			if len(body) > 0 {
				return "created-with-body", nil
			}
			return "created-empty", nil
		}
		if status == 204 {
			return "no-content", nil
		}
		return "success-other", nil
	}

	if status < 400 {
		if status == 301 || status == 302 {
			return "redirect", nil
		}
		return "redirect-other", nil
	}

	if status < 500 {
		if status == 401 || status == 403 {
			return "auth-error", nil
		}
		return "client-error", nil
	}

	return "server-error", nil
}

// ProcessStateMachine runs a simple state machine through a sequence of events.
// States: idle -> loading -> (success | error) -> done
// The solver must generate valid event sequences to reach each state.
func ProcessStateMachine(events []string, maxRetries int) string {
	type state int
	const (
		idle state = iota
		loading
		success
		errState
		done
	)

	current := idle
	retries := 0

	if len(events) == 0 {
		return "idle"
	}

	for _, event := range events {
		switch current {
		case idle:
			if event == "start" {
				current = loading
			} else {
				return "invalid-transition"
			}

		case loading:
			if event == "success" {
				current = success
			} else if event == "error" {
				retries++
				if retries <= maxRetries {
					current = loading
				} else {
					current = errState
				}
			} else {
				return "invalid-transition"
			}

		case success:
			if event == "reset" {
				current = done
			} else {
				return "invalid-transition"
			}

		case errState:
			if event == "reset" {
				current = done
			} else {
				return "invalid-transition"
			}

		case done:
			return "done"
		}
	}

	stateNames := map[state]string{
		idle:     "idle",
		loading:  "loading",
		success:  "success",
		errState: "error",
		done:     "done",
	}
	return stateNames[current]
}
