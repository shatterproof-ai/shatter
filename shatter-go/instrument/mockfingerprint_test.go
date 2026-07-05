package instrument

import (
	"testing"

	"pgregory.net/rapid"
)

func mockConfigGen() *rapid.Generator[MockConfig] {
	return rapid.Custom(func(t *rapid.T) MockConfig {
		return MockConfig{
			Symbol:           rapid.StringMatching(`[a-z]{1,8}(\.[A-Z][a-z]{0,6})?`).Draw(t, "symbol"),
			Expression:       rapid.StringN(0, 12, 24).Draw(t, "expression"),
			DefaultBehavior:  rapid.SampledFrom([]string{"", "zero", "passthrough"}).Draw(t, "behavior"),
			ShouldTrackCalls: rapid.Bool().Draw(t, "track"),
			ReturnValues: rapid.SliceOfN(rapid.Int().AsAny(), 0, 3).Draw(t, "returns"),
		}
	})
}

// MockFingerprint is the single source of truth for mock-sensitive cache keys
// (build.cacheKey and computePrepareID). Properties: deterministic,
// order-independent, and sensitive to every field — losing any of these
// silently reuses stale launcher binaries / prepared harnesses across
// differing mock sets (str-c8djq).
func TestProperty_MockFingerprint(t *testing.T) {
	t.Run("deterministic and order-independent", func(t *testing.T) {
		rapid.Check(t, func(t *rapid.T) {
			mocks := rapid.SliceOfN(mockConfigGen(), 0, 5).Draw(t, "mocks")
			a := MockFingerprint(mocks)
			if b := MockFingerprint(mocks); b != a {
				t.Fatalf("not deterministic: %q vs %q", a, b)
			}
			perm := rapid.Permutation(mocks).Draw(t, "perm")
			if b := MockFingerprint(perm); b != a {
				t.Fatalf("order-dependent: %q vs %q", a, b)
			}
		})
	})
	t.Run("sensitive to each field", func(t *testing.T) {
		rapid.Check(t, func(t *rapid.T) {
			mocks := rapid.SliceOfN(mockConfigGen(), 1, 4).Draw(t, "mocks")
			base := MockFingerprint(mocks)
			i := rapid.IntRange(0, len(mocks)-1).Draw(t, "idx")
			mutated := append([]MockConfig{}, mocks...)
			switch rapid.IntRange(0, 3).Draw(t, "field") {
			case 0:
				mutated[i].Symbol += "X"
			case 1:
				mutated[i].Expression += "X"
			case 2:
				mutated[i].DefaultBehavior += "X"
			case 3:
				mutated[i].ShouldTrackCalls = !mutated[i].ShouldTrackCalls
			}
			if MockFingerprint(mutated) == base {
				t.Fatalf("fingerprint insensitive to field mutation at %d", i)
			}
		})
	})
}
