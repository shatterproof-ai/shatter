// Package color is the str-pjlc1 end-to-end fixture for enum value-domain
// extraction. `Color` is a named string type ("enum") with a three-value const
// set; `ClassifyColor` switches over all three plus a default arm. Without a
// value domain the generator only produces generic strings ("a"/"hello") that
// all fall to the default arm, so the three valid arms are never reached. With
// enum_values carried on the param's union TypeInfo, the core draws RED/GREEN/
// BLUE and reaches every arm.
package color

// Color is a named string enum with a constant set.
type Color string

// The three-value const set that forms Color's value domain.
const (
	ColorRed   Color = "RED"
	ColorGreen Color = "GREEN"
	ColorBlue  Color = "BLUE"
)

// ClassifyColor switches over the Color enum with an explicit default arm.
// Four reachable return values: the three valid members plus the off-domain
// default probe.
func ClassifyColor(c Color) string {
	switch c {
	case ColorRed:
		return "warm"
	case ColorGreen:
		return "cool-green"
	case ColorBlue:
		return "cool-blue"
	default:
		return "invalid"
	}
}
