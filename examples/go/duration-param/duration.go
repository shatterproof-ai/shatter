// Package durationparam is the str-is5g E2E fixture for the Go frontend's
// time.Duration parameter path. The canonical wire format is integer
// nanoseconds (time.Duration is an int64 alias); see the wrapper helper
// writeDurationParamDeserialization for the legacy object-form fallback.
//
// Four reachable branches cover zero, positive sub-second, positive
// at-or-above-one-second, and negative durations.
package durationparam

import "time"

// Categorize returns a code based on the timeout:
//
//	timeout <  0           -> -1
//	timeout == 0           ->  0
//	0 <  timeout <  1s     ->  1
//	timeout >= time.Second ->  2
func Categorize(timeout time.Duration) int {
	if timeout < 0 {
		return -1
	}
	if timeout == 0 {
		return 0
	}
	if timeout < time.Second {
		return 1
	}
	return 2
}
