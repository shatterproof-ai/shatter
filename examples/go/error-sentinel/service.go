// Package errorsentinel is the str-kvzh7 E2E fixture for the Go frontend's
// errors.Is sentinel-mining path. RunErrorMessage mirrors kapow's runErrorMessage
// (api/internal/chat/service.go): it switches an error-typed parameter on two
// imported sentinel comparisons plus a default arm.
//
// Before str-kvzh7 a synthesized error param could only be nil or
// errors.New(message); neither satisfies errors.Is(err, nl.ErrX), so only the
// default arm was reachable. The analyzer now mines nl.ErrQuestionRequired and
// nl.ErrAnswerRequired as sentinel targets, the wrapper bakes them into a
// []error table selected by an {"__complex_type":"error","sentinel":N} input,
// and the planner emits one such selector per sentinel — so all three arms
// become reachable through the concolic seed path.
package errorsentinel

import (
	"errors"

	"example.com/error-sentinel/nl"
)

// RunErrorMessage classifies err by sentinel identity.
//
//	errors.Is(err, nl.ErrQuestionRequired) -> "question"
//	errors.Is(err, nl.ErrAnswerRequired)   -> "answer"
//	otherwise (nil or any other error)     -> "default"
func RunErrorMessage(err error) string {
	if errors.Is(err, nl.ErrQuestionRequired) {
		return "question"
	}
	if errors.Is(err, nl.ErrAnswerRequired) {
		return "answer"
	}
	return "default"
}
