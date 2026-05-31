package wazero

import "context"

type Runtime interface {
	Close(context.Context) error
}
