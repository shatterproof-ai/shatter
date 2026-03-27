package timing

import (
	"testing"
	"time"
)

func TestSinglePhase(t *testing.T) {
	c := NewCollector()
	finish := c.Start("analyze.parse")
	time.Sleep(time.Millisecond)
	finish()

	s := c.Summary()
	if s == nil {
		t.Fatal("Summary should not be nil after recording a phase")
	}
	if len(s.Phases) != 1 {
		t.Fatalf("got %d phases, want 1", len(s.Phases))
	}
	p := s.Phases[0]
	if p.PhasePath != "analyze.parse" {
		t.Errorf("PhasePath = %q, want %q", p.PhasePath, "analyze.parse")
	}
	if p.TotalMs <= 0 {
		t.Errorf("TotalMs = %f, want > 0", p.TotalMs)
	}
	if p.SelfMs <= 0 {
		t.Errorf("SelfMs = %f, want > 0", p.SelfMs)
	}
	if p.Count != 1 {
		t.Errorf("Count = %d, want 1", p.Count)
	}
}

func TestNestedPhases(t *testing.T) {
	c := NewCollector()
	finishParent := c.Start("analyze.total")
	time.Sleep(time.Millisecond)

	finishChild := c.Start("analyze.parse")
	time.Sleep(2 * time.Millisecond)
	finishChild()

	finishParent()

	s := c.Summary()
	if s == nil {
		t.Fatal("Summary should not be nil")
	}

	phases := make(map[string]PhaseSummary)
	for _, p := range s.Phases {
		phases[p.PhasePath] = p
	}

	parent, ok := phases["analyze.total"]
	if !ok {
		t.Fatal("missing analyze.total phase")
	}
	child, ok := phases["analyze.parse"]
	if !ok {
		t.Fatal("missing analyze.parse phase")
	}

	// Parent's self_ms should be less than total_ms (child time subtracted).
	if parent.SelfMs >= parent.TotalMs {
		t.Errorf("parent SelfMs (%f) should be < TotalMs (%f)", parent.SelfMs, parent.TotalMs)
	}
	// Child's total should be roughly its self (no grandchildren).
	if child.SelfMs != child.TotalMs {
		t.Errorf("leaf child SelfMs (%f) should equal TotalMs (%f)", child.SelfMs, child.TotalMs)
	}
}

func TestAggregation(t *testing.T) {
	c := NewCollector()
	for i := 0; i < 3; i++ {
		finish := c.Start("execute.run")
		time.Sleep(time.Millisecond)
		finish()
	}

	s := c.Summary()
	if s == nil {
		t.Fatal("Summary should not be nil")
	}
	if len(s.Phases) != 1 {
		t.Fatalf("got %d phases, want 1 (aggregated)", len(s.Phases))
	}
	p := s.Phases[0]
	if p.Count != 3 {
		t.Errorf("Count = %d, want 3", p.Count)
	}
}

func TestEmptyCollector(t *testing.T) {
	c := NewCollector()
	if s := c.Summary(); s != nil {
		t.Errorf("empty collector Summary should be nil, got %+v", s)
	}
}

func TestNilCollector(t *testing.T) {
	var c *Collector
	// Start on nil should return a no-op closure without panicking.
	finish := c.Start("anything")
	finish()

	if s := c.Summary(); s != nil {
		t.Errorf("nil collector Summary should be nil, got %+v", s)
	}
}

func TestSortedOutput(t *testing.T) {
	c := NewCollector()
	for _, name := range []string{"c.third", "a.first", "b.second"} {
		finish := c.Start(name)
		finish()
	}

	s := c.Summary()
	if s == nil {
		t.Fatal("Summary should not be nil")
	}
	if len(s.Phases) != 3 {
		t.Fatalf("got %d phases, want 3", len(s.Phases))
	}
	for i := 1; i < len(s.Phases); i++ {
		if s.Phases[i].PhasePath < s.Phases[i-1].PhasePath {
			t.Errorf("phases not sorted: %q comes after %q", s.Phases[i].PhasePath, s.Phases[i-1].PhasePath)
		}
	}
}

func TestSelfMsNonNegative(t *testing.T) {
	// Even with fast nested phases, SelfMs should never go negative.
	c := NewCollector()
	finishOuter := c.Start("outer")
	finishInner := c.Start("inner")
	finishInner()
	finishOuter()

	s := c.Summary()
	for _, p := range s.Phases {
		if p.SelfMs < 0 {
			t.Errorf("phase %q has negative SelfMs: %f", p.PhasePath, p.SelfMs)
		}
	}
}
