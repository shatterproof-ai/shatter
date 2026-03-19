package timing

import (
	"sort"
	"time"
)

type activePhase struct {
	phasePath string
	start     time.Time
	childMs   float64
}

// Summary captures aggregated phase timings for a frontend command.
type Summary struct {
	Phases []PhaseSummary `json:"phases,omitempty"`
}

// PhaseSummary captures one named timing phase.
type PhaseSummary struct {
	PhasePath  string            `json:"phase_path"`
	TotalMs    float64           `json:"total_ms"`
	SelfMs     float64           `json:"self_ms"`
	Count      int               `json:"count"`
	Attributes map[string]string `json:"attributes,omitempty"`
}

// Collector aggregates nested timing phases for a single frontend request.
type Collector struct {
	active []activePhase
	phases map[string]*PhaseSummary
}

// NewCollector constructs an empty timing collector.
func NewCollector() *Collector {
	return &Collector{
		phases: make(map[string]*PhaseSummary),
	}
}

// Start begins a named phase and returns a closure that records its completion.
// It is safe to call on a nil collector; the returned closure will be a no-op.
func (c *Collector) Start(phasePath string) func() {
	if c == nil {
		return func() {}
	}

	phase := activePhase{
		phasePath: phasePath,
		start:     time.Now(),
	}
	c.active = append(c.active, phase)

	return func() {
		c.finish(phasePath)
	}
}

// Summary returns the aggregated timing summary, or nil when nothing was recorded.
func (c *Collector) Summary() *Summary {
	if c == nil || len(c.phases) == 0 {
		return nil
	}

	phases := make([]PhaseSummary, 0, len(c.phases))
	for _, phase := range c.phases {
		phases = append(phases, *phase)
	}
	sort.Slice(phases, func(i, j int) bool {
		return phases[i].PhasePath < phases[j].PhasePath
	})
	return &Summary{Phases: phases}
}

func (c *Collector) finish(phasePath string) {
	if len(c.active) == 0 {
		panic("timing phase stack underflow")
	}

	idx := len(c.active) - 1
	phase := c.active[idx]
	c.active = c.active[:idx]
	if phase.phasePath != phasePath {
		panic("timing phase stack mismatch for " + phasePath)
	}

	totalMs := float64(time.Since(phase.start).Microseconds()) / 1000.0
	selfMs := totalMs - phase.childMs
	if selfMs < 0 {
		selfMs = 0
	}

	existing := c.phases[phasePath]
	if existing != nil {
		existing.TotalMs += totalMs
		existing.SelfMs += selfMs
		existing.Count++
	} else {
		c.phases[phasePath] = &PhaseSummary{
			PhasePath: phasePath,
			TotalMs:   totalMs,
			SelfMs:    selfMs,
			Count:     1,
		}
	}

	if len(c.active) > 0 {
		c.active[len(c.active)-1].childMs += totalMs
	}
}
