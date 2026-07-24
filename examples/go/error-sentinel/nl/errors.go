// Package nl holds exported sentinel errors, mirroring a domain error package
// (e.g. kapow's natural-language layer) whose sentinels are compared with
// errors.Is in consuming code.
package nl

import "errors"

// ErrQuestionRequired and ErrAnswerRequired are exported package-level sentinel
// errors. errors.Is comparisons against them require pointer identity, so only
// the sentinel value itself — not errors.New(msg) — can satisfy the branch.
var (
	ErrQuestionRequired = errors.New("question required")
	ErrAnswerRequired   = errors.New("answer required")
)
