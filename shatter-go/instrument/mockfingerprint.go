package instrument

import (
	"encoding/json"
	"sort"
	"strconv"
	"strings"
)

// MockFingerprint returns a deterministic, order-independent canonical string
// capturing every field of a mock set that changes the generated harness or
// substituted source: Symbol, Expression, DefaultBehavior, ShouldTrackCalls,
// and ReturnValues. It is the single source of truth for mock-sensitive cache
// keys — both build.cacheKey (launcher binary cache) and computePrepareID
// (prepared-harness cache) feed it into their hashes so a change in any mock
// field invalidates both caches (str-c8djq review fix 3). Including
// ReturnValues is essential: the prepare fast path keys on this before Build
// runs, so omitting them would reuse a stale harness across different
// return-value tables.
func MockFingerprint(mocks []MockConfig) string {
	if len(mocks) == 0 {
		return ""
	}
	parts := make([]string, 0, len(mocks))
	for _, m := range mocks {
		rv, _ := json.Marshal(m.ReturnValues)
		parts = append(parts, strings.Join([]string{
			m.Symbol,
			m.Expression,
			m.DefaultBehavior,
			strconv.FormatBool(m.ShouldTrackCalls),
			string(rv),
		}, "\x1f"))
	}
	sort.Strings(parts)
	return strings.Join(parts, "\x1e")
}
