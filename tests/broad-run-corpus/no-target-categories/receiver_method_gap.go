// NoTargetReason::ReceiverMethodGap — file declares only methods on a
// receiver type that the analyzer cannot synthesize an executable target
// for (Go-specific). No top-level functions, no public constructor.
package broadrun

type opaqueState struct {
	tag int
}

func (o *opaqueState) Tag() int {
	return o.tag
}

func (o *opaqueState) WithTag(tag int) *opaqueState {
	o.tag = tag
	return o
}
