package build

import (
	"strings"
	"testing"
)

func TestRuntimeHelperGeneratesDeterministicCryptoRandRead(t *testing.T) {
	src := generateRuntimeHelper("targetpkg", nil, false)
	if !strings.Contains(src, "func __shatter_side_effect_crypto_rand_read(fn func([]byte) (int, error), buf []byte) (int, error)") {
		t.Fatalf("runtime helper missing crypto/rand.Read helper:\n%s", src)
	}
	if !strings.Contains(src, "buf[i] = byte((i*31 + 17) & 0xff)") {
		t.Fatalf("crypto/rand.Read helper does not fill bytes deterministically:\n%s", src)
	}
	if strings.Contains(src, "return fn(buf)") {
		t.Fatalf("crypto/rand.Read helper must not pass through to real randomness:\n%s", src)
	}
}
