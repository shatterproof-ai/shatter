package instrument

import (
	"math"
	"testing"

	"pgregory.net/rapid"
)

func TestShannonEntropyConstantBytes(t *testing.T) {
	data := make([]byte, 256)
	for i := range data {
		data[i] = 0x42
	}
	e := ShannonEntropy(data)
	if e != 0.0 {
		t.Fatalf("expected 0.0 for constant bytes, got %f", e)
	}
}

func TestShannonEntropyUniform(t *testing.T) {
	data := make([]byte, 256)
	for i := range data {
		data[i] = byte(i)
	}
	e := ShannonEntropy(data)
	if math.Abs(e-8.0) > 0.001 {
		t.Fatalf("expected ~8.0 for uniform distribution, got %f", e)
	}
}

func TestShannonEntropyTwoValues(t *testing.T) {
	data := make([]byte, 256)
	// first half = 0, second half = 1
	for i := 128; i < 256; i++ {
		data[i] = 1
	}
	e := ShannonEntropy(data)
	if math.Abs(e-1.0) > 0.001 {
		t.Fatalf("expected ~1.0 for two values, got %f", e)
	}
}

func TestShannonEntropyEmpty(t *testing.T) {
	if e := ShannonEntropy(nil); e != 0.0 {
		t.Fatalf("expected 0.0 for nil, got %f", e)
	}
	if e := ShannonEntropy([]byte{}); e != 0.0 {
		t.Fatalf("expected 0.0 for empty, got %f", e)
	}
}

func TestShannonEntropyBelowMinBuffer(t *testing.T) {
	data := make([]byte, MinBufferSize-1)
	if e := ShannonEntropy(data); e != 0.0 {
		t.Fatalf("expected 0.0 for short buffer, got %f", e)
	}
}

func TestShannonEntropyExactlyMinBuffer(t *testing.T) {
	data := make([]byte, MinBufferSize)
	for i := range data {
		data[i] = byte(i)
	}
	if e := ShannonEntropy(data); e <= 0.0 {
		t.Fatalf("expected >0 for min buffer with varied data, got %f", e)
	}
}

func TestPropertyEntropyInRange(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		data := rapid.SliceOf(rapid.Byte()).Draw(t, "data")
		e := ShannonEntropy(data)
		if e < 0.0 || e > 8.0 {
			t.Fatalf("entropy %f out of range [0, 8]", e)
		}
	})
}

func TestPropertyShortBuffersZero(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		n := rapid.IntRange(0, MinBufferSize-1).Draw(t, "n")
		data := make([]byte, n)
		for i := range data {
			data[i] = rapid.Byte().Draw(t, "byte")
		}
		if e := ShannonEntropy(data); e != 0.0 {
			t.Fatalf("expected 0.0 for short buffer (len=%d), got %f", n, e)
		}
	})
}
