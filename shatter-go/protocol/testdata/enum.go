package testdata

// Color is a named string enum with a three-value const set plus a switch that
// has an explicit default arm — the str-pjlc1 acceptance shape (3 valid arms +
// 1 off-domain probe arm).
type Color string

const (
	ColorRed   Color = "RED"
	ColorGreen Color = "GREEN"
	ColorBlue  Color = "BLUE"
)

// ClassifyColor switches over the Color enum with a default arm.
func ClassifyColor(c Color) string {
	switch c {
	case ColorRed:
		return "warm"
	case ColorGreen:
		return "cool-green"
	case ColorBlue:
		return "cool-blue"
	default:
		return "unknown"
	}
}

// Priority is a named integer enum declared with iota.
type Priority int

const (
	PriorityLow Priority = iota
	PriorityMedium
	PriorityHigh
)

// ClassifyPriority branches on the Priority integer enum.
func ClassifyPriority(p Priority) string {
	switch p {
	case PriorityHigh:
		return "urgent"
	case PriorityMedium:
		return "soon"
	default:
		return "whenever"
	}
}

// Flag is a named unsigned enum whose top constant exceeds math.MaxInt64 —
// the extraction must use Uint64Val (Int64Val drops it) and the base variant
// must be the unsigned go_uint kind so off-domain probes stay non-negative.
type Flag uint64

const (
	FlagNone Flag = 0
	FlagSome Flag = 2
	FlagHigh Flag = 1 << 63 // 9223372036854775808 > math.MaxInt64
)

// ClassifyFlag branches on the unsigned Flag enum.
func ClassifyFlag(f Flag) string {
	switch f {
	case FlagHigh:
		return "high"
	case FlagSome:
		return "some"
	default:
		return "none"
	}
}

// Bare is a named string type with NO constants of its own — it must remain a
// plain string, not a union, so ordinary string generation still applies.
type Bare string

// AcceptBare takes a constant-free named string type.
func AcceptBare(b Bare) string {
	return string(b)
}
