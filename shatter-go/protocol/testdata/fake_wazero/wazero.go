package wazero

import "context"

type Runtime interface {
	CompileModule(context.Context, []byte) (CompiledModule, error)
	Close(context.Context) error
}

type CompiledModule interface {
	Close(context.Context) error
}
