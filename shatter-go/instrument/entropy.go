// Package instrument provides runtime instrumentation for the Go frontend.

package instrument

import "math"

// MinBufferSize is the minimum byte count for reliable entropy measurement.
const MinBufferSize = 32

// EntropyDeltaThreshold is the minimum bits/byte difference between input
// and output entropy to classify as likely crypto.
const EntropyDeltaThreshold = 1.5

// ShannonEntropy computes Shannon entropy in bits per byte (0.0 to 8.0).
// Returns 0.0 for slices shorter than MinBufferSize.
func ShannonEntropy(data []byte) float64 {
	if len(data) < MinBufferSize {
		return 0.0
	}

	var counts [256]uint64
	for _, b := range data {
		counts[b]++
	}

	length := float64(len(data))
	entropy := 0.0
	for _, count := range counts {
		if count > 0 {
			p := float64(count) / length
			entropy -= p * math.Log2(p)
		}
	}

	return entropy
}
