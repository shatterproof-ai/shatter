package reconstruct

import (
	"math/big"
	"net"
	"net/url"
	"regexp"
	"testing"
	"time"
)

func TestDateReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "date",
		"value":          float64(1704067200000),
	}
	result := Value(input)
	tm, ok := result.(time.Time)
	if !ok {
		t.Fatalf("expected time.Time, got %T", result)
	}
	expected := time.UnixMilli(1704067200000)
	if !tm.Equal(expected) {
		t.Errorf("expected %v, got %v", expected, tm)
	}
}

func TestDurationReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "duration",
		"ms":             float64(3600000),
	}
	result := Value(input)
	d, ok := result.(time.Duration)
	if !ok {
		t.Fatalf("expected time.Duration, got %T", result)
	}
	if d != time.Hour {
		t.Errorf("expected 1h, got %v", d)
	}
}

func TestURLReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "url",
		"value":          "https://example.com/path?q=1",
	}
	result := Value(input)
	u, ok := result.(*url.URL)
	if !ok {
		t.Fatalf("expected *url.URL, got %T", result)
	}
	if u.Host != "example.com" {
		t.Errorf("expected host example.com, got %s", u.Host)
	}
}

func TestRegExpReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "reg_exp",
		"source":         `\d+`,
	}
	result := Value(input)
	re, ok := result.(*regexp.Regexp)
	if !ok {
		t.Fatalf("expected *regexp.Regexp, got %T", result)
	}
	if !re.MatchString("123") {
		t.Error("expected regexp to match '123'")
	}
}

func TestIPAddressReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "ip_address",
		"value":          "192.168.1.1",
	}
	result := Value(input)
	ip, ok := result.(net.IP)
	if !ok {
		t.Fatalf("expected net.IP, got %T", result)
	}
	if ip.String() != "192.168.1.1" {
		t.Errorf("expected 192.168.1.1, got %s", ip.String())
	}
}

func TestBigIntReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "big_int",
		"value":          "99999999999999999999",
	}
	result := Value(input)
	n, ok := result.(*big.Int)
	if !ok {
		t.Fatalf("expected *big.Int, got %T", result)
	}
	expected, _ := new(big.Int).SetString("99999999999999999999", 10)
	if n.Cmp(expected) != 0 {
		t.Errorf("expected %v, got %v", expected, n)
	}
}

func TestErrorReconstruction(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "error",
		"message":        "bad input",
	}
	result := Value(input)
	err, ok := result.(error)
	if !ok {
		t.Fatalf("expected error, got %T", result)
	}
	if err.Error() != "bad input" {
		t.Errorf("expected 'bad input', got '%s'", err.Error())
	}
}

func TestPlainValuePassthrough(t *testing.T) {
	// Non-map values should pass through
	if v := Value(42); v != 42 {
		t.Errorf("expected 42, got %v", v)
	}
	if v := Value("hello"); v != "hello" {
		t.Errorf("expected hello, got %v", v)
	}
}

func TestPlainMapRecursion(t *testing.T) {
	// Maps without __complex_type should recurse into fields
	input := map[string]interface{}{
		"nested": map[string]interface{}{
			"__complex_type": "date",
			"value":          float64(0),
		},
	}
	result := Value(input)
	m, ok := result.(map[string]interface{})
	if !ok {
		t.Fatalf("expected map, got %T", result)
	}
	if _, ok := m["nested"].(time.Time); !ok {
		t.Errorf("expected nested date to be reconstructed, got %T", m["nested"])
	}
}

func TestInputsReconstruction(t *testing.T) {
	inputs := []interface{}{
		map[string]interface{}{
			"__complex_type": "date",
			"value":          float64(1704067200000),
		},
		"plain string",
		float64(42),
	}
	result := Inputs(inputs)
	if len(result) != 3 {
		t.Fatalf("expected 3 results, got %d", len(result))
	}
	if _, ok := result[0].(time.Time); !ok {
		t.Errorf("expected time.Time for first input, got %T", result[0])
	}
	if result[1] != "plain string" {
		t.Errorf("expected 'plain string', got %v", result[1])
	}
}

func TestUnknownComplexTypePassthrough(t *testing.T) {
	input := map[string]interface{}{
		"__complex_type": "some_future_type",
		"value":          "x",
	}
	result := Value(input)
	m, ok := result.(map[string]interface{})
	if !ok {
		t.Fatalf("expected map passthrough, got %T", result)
	}
	if m["__complex_type"] != "some_future_type" {
		t.Error("expected passthrough to preserve tag")
	}
}
