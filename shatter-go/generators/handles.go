package generators

import (
	"fmt"
	"sync"
)

// HandleTable stores live (non-serializable) objects returned by native
// generators, keyed by an opaque handle ID. The core engine receives a
// sentinel value referencing the handle; when the frontend needs the live
// object for execution, it resolves the handle here.
type HandleTable struct {
	mu      sync.Mutex
	handles map[string]any
	nextID  int
}

// NewHandleTable returns an empty handle table.
func NewHandleTable() *HandleTable {
	return &HandleTable{handles: make(map[string]any)}
}

// Store saves val and returns a unique handle ID.
func (t *HandleTable) Store(val any) string {
	t.mu.Lock()
	defer t.mu.Unlock()

	t.nextID++
	id := fmt.Sprintf("h_%04d", t.nextID)
	t.handles[id] = val
	return id
}

// Resolve returns the live object for id, or nil if not found.
func (t *HandleTable) Resolve(id string) any {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.handles[id]
}

// Clear removes all stored handles. Call between exploration runs.
func (t *HandleTable) Clear() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.handles = make(map[string]any)
	t.nextID = 0
}

// Len returns the number of stored handles.
func (t *HandleTable) Len() int {
	t.mu.Lock()
	defer t.mu.Unlock()
	return len(t.handles)
}
