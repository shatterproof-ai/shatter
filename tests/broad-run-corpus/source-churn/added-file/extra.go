package broadrunchurn

// ExtraTrend is added between phase 1 and phase 2 by the gate driver. The
// run manifest's source snapshot should detect that the file set changed
// between the two invocations (str-jeen.3).
func ExtraTrend(value int) string {
	if value > 100 {
		return "spike"
	}
	return "steady"
}
