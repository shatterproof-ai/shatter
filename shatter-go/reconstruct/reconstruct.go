// Package reconstruct converts __complex_type tagged JSON into Go native values.
//
// Called on input values before passing them to the function under test.
// Each frontend has its own reconstruction module matching the types it declared
// in its handshake capabilities.
package reconstruct

import (
	"math/big"
	"net"
	"net/url"
	"regexp"
	"time"
)

// Value reconstructs a __complex_type tagged JSON map into a native Go value.
// Non-map values and maps without the tag are returned unchanged.
func Value(v interface{}) interface{} {
	m, ok := v.(map[string]interface{})
	if !ok {
		return v
	}

	tag, ok := m["__complex_type"].(string)
	if !ok {
		// Plain object: reconstruct each field
		result := make(map[string]interface{}, len(m))
		for k, val := range m {
			result[k] = Value(val)
		}
		return result
	}

	switch tag {
	case "date", "date_time":
		if ms, ok := toInt64(m["value"]); ok {
			return time.UnixMilli(ms)
		}
		return v

	case "duration":
		raw := m["ms"]
		if raw == nil {
			raw = m["value"]
		}
		if ms, ok := toInt64(raw); ok {
			return time.Duration(ms) * time.Millisecond
		}
		return v

	case "url":
		if s, ok := m["value"].(string); ok {
			if u, err := url.Parse(s); err == nil {
				return u
			}
		}
		return v

	case "reg_exp":
		if source, ok := m["source"].(string); ok {
			if re, err := regexp.Compile(source); err == nil {
				return re
			}
		}
		return v

	case "ip_address":
		if s, ok := m["value"].(string); ok {
			return net.ParseIP(s)
		}
		return v

	case "big_int":
		if s, ok := m["value"].(string); ok {
			n := new(big.Int)
			if _, ok := n.SetString(s, 10); ok {
				return n
			}
		}
		return v

	case "rational":
		if s, ok := m["value"].(string); ok {
			r := new(big.Rat)
			if _, ok := r.SetString(s); ok {
				return r
			}
		}
		return v

	case "big_decimal":
		if s, ok := m["value"].(string); ok {
			f := new(big.Float)
			if _, ok := f.SetString(s); ok {
				return f
			}
		}
		return v

	case "error":
		if msg, ok := m["message"].(string); ok {
			return errorString(msg)
		}
		return v

	default:
		// Unknown complex type: pass through as-is
		return v
	}
}

// Inputs reconstructs a slice of input values.
func Inputs(inputs []interface{}) []interface{} {
	result := make([]interface{}, len(inputs))
	for i, v := range inputs {
		result[i] = Value(v)
	}
	return result
}

// toInt64 converts a JSON number (float64) to int64.
func toInt64(v interface{}) (int64, bool) {
	switch n := v.(type) {
	case float64:
		return int64(n), true
	case int64:
		return n, true
	default:
		return 0, false
	}
}

// errorString is a simple error implementation for reconstructed errors.
type errorString string

func (e errorString) Error() string { return string(e) }
