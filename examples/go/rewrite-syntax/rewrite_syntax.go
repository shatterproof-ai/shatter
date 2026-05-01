// Package rewritesyntax is a fixture for the Go-frontend rewrite/syntax
// regression test (str-jeen.34). It is deliberately wide rather than
// deep: each top-level declaration exercises a distinct AST shape that
// the instrumentor's visitor must rewrite without producing unparseable
// Go output. See README.md for the full list of constructs covered.
package rewritesyntax

import (
	"fmt"
	"strings"
)

// Numeric is a minimal type set for the generic helper below.
type Numeric interface {
	~int | ~int32 | ~int64 | ~float32 | ~float64
}

// Counter is a value type with both pointer- and value-receiver methods.
// The instrumentor must not collapse method receivers when rewriting.
type Counter struct {
	value int
	tags  []string
}

// Embeds combines an embedded value-type and an embedded interface, both
// of which must round-trip through the rewriter unchanged.
type Embeds struct {
	Counter
	fmt.Stringer
	meta map[string]any
}

// SumGeneric is a single-type-parameter generic with a type set
// constraint. The rewriter must preserve the type-parameter list.
func SumGeneric[T Numeric](xs []T) T {
	var total T
	for _, x := range xs {
		total += x
	}
	return total
}

// PairGeneric is a multi-type-parameter generic; the constraint includes
// `comparable`, which is a predeclared identifier rather than a named
// interface — historically a source of rewrite confusion.
func PairGeneric[K comparable, V any](key K, value V) map[K]V {
	out := make(map[K]V, 1)
	if key == *new(K) {
		return out
	}
	out[key] = value
	return out
}

// IncrementBy mutates the receiver via a pointer, then defers a tag
// append. Exercises pointer receivers, defer with a bound argument, and
// reassignment of a captured parameter after a closure construction.
func (c *Counter) IncrementBy(delta int, tag string) (newValue int, err error) {
	if c == nil {
		return 0, fmt.Errorf("nil counter")
	}
	defer func(t string) {
		c.tags = append(c.tags, t)
	}(tag)

	apply := func() {
		c.value += delta
	}
	// Reassign delta after the closure is constructed; the visitor's
	// `isReassignedAfter` guard must not produce broken output here.
	if delta < 0 {
		delta = -delta
	}
	apply()
	return c.value, nil
}

// Describe is a value-receiver method returning multiple values via a
// named-return signature. Type switch with an init clause exercises
// `transformSwitchStmt` against `*ast.TypeSwitchStmt` shape.
func (c Counter) Describe(extra any) (label string, ok bool) {
	switch v := extra.(type) {
	case nil:
		label, ok = "nil", false
	case int:
		label, ok = fmt.Sprintf("int=%d", v), true
	case string:
		label, ok = "str="+strings.ToUpper(v), true
	case []byte:
		label, ok = fmt.Sprintf("bytes=%d", len(v)), true
	default:
		label, ok = fmt.Sprintf("other=%T", v), false
	}
	return
}

// FanOut spawns a goroutine that sends results into a buffered channel,
// then drains it with `<-ch`. This exercises both send (`ch <- v`) and
// receive (`<-ch`) operators — the receive form previously caused
// `buildUnOp` to emit a synthetic binary-op name (str-gq7c).
func FanOut(items []string, prefix string) []string {
	ch := make(chan string, len(items))
	go func(src []string) {
		for _, item := range src {
			ch <- prefix + item
		}
		close(ch)
	}(items)

	out := make([]string, 0, len(items))
	for v := range ch {
		out = append(out, v)
	}
	// Explicit receive form: `<-` as a unary expression.
	select {
	case extra, more := <-ch:
		if more {
			out = append(out, extra)
		}
	default:
	}
	return out
}

// Variadic exercises variadic parameters and address-of (`&x`).
// Address-of as a unary expression is the canonical str-gq7c case.
func Variadic(seed int, more ...int) *int {
	total := seed
	for _, n := range more {
		total += n
	}
	if total < 0 {
		total = -total
	}
	// `&total` — address-of — must round-trip cleanly.
	p := &total
	*p++
	return p
}

// AnonymousStruct returns a slice of anonymous struct values; the
// rewriter must preserve composite-literal type expressions.
func AnonymousStruct(n int) []struct {
	Index int
	Label string
} {
	out := make([]struct {
		Index int
		Label string
	}, 0, n)
	for i := 0; i < n; i++ {
		if i%2 == 0 {
			out = append(out, struct {
				Index int
				Label string
			}{Index: i, Label: fmt.Sprintf("even-%d", i)})
		} else {
			out = append(out, struct {
				Index int
				Label string
			}{Index: i, Label: fmt.Sprintf("odd-%d", i)})
		}
	}
	return out
}

// MapAndChan exercises range-over-map plus a nested closure inside a
// for-range loop body, which requires `instrumentFuncLits` to operate
// on closures inside `transformBlock`.
func MapAndChan(in map[string]int) map[string]int {
	out := make(map[string]int, len(in))
	apply := func(k string, v int) {
		if v > 0 {
			out[k] = v * 2
		}
	}
	for k, v := range in {
		apply(k, v)
	}
	return out
}
