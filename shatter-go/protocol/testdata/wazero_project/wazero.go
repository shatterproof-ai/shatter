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

// AcceptsCompiledModule takes a live compiled WASM module. The analyzer should
// preserve the canonical spelling so the runtime-value registry can supply one.
func AcceptsCompiledModule(mod wazero.CompiledModule) error {
	return mod.Close(context.Background())
}

type Runner struct {
	rt wazero.Runtime
}

type Generator struct {
	compiled wazero.CompiledModule
}

func NewGenerator(compiled wazero.CompiledModule) Generator {
	return Generator{compiled: compiled}
}

func NewRunner(rt wazero.Runtime) Runner {
	return Runner{rt: rt}
}

func AcceptsRunner(r Runner) bool {
	return r.rt != nil
}

func AcceptsGenerator(g Generator) bool {
	return g.compiled != nil
}
