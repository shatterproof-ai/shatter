package wazero_project

import (
	"context"

	"github.com/tetratelabs/wazero"
)

// AcceptsWazeroRuntime takes a live wazero runtime. The analyzer should treat
// this as a synthesizable runtime value rather than descending into wazero's
// internal implementation fields.
func AcceptsWazeroRuntime(rt wazero.Runtime) error {
	return rt.Close(context.Background())
}

type Runner struct {
	rt wazero.Runtime
}

func NewRunner(rt wazero.Runtime) Runner {
	return Runner{rt: rt}
}

func AcceptsRunner(r Runner) bool {
	return r.rt != nil
}
