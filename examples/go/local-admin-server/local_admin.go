// Package localadmin mirrors the shape of a local admin/control-plane file
// that clusters build and task timeouts during a Shatter scan (str-slnr).
//
// The Zolem original (cmd/zolem/local_admin.go) pairs an HTTP local server
// lifecycle type holding a net.Listener with a mutex-guarded "local control
// plane" type and a handful of small free helpers. Every target in the file
// shares one launcher build, so a build that outruns --build-timeout fails the
// whole cluster rather than one function.
package localadmin

import (
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"strings"
	"sync"
)

// httpLocalServer owns a listener plus the http.Server driving it.
type httpLocalServer struct {
	listener net.Listener
	server   *http.Server
}

// Addr reports the bound address, or the empty string before Listen.
func (s *httpLocalServer) Addr() string {
	if s == nil || s.listener == nil {
		return ""
	}
	return s.listener.Addr().String()
}

// Close shuts the listener down, tolerating an unstarted server.
func (s *httpLocalServer) Close() error {
	if s == nil || s.listener == nil {
		return nil
	}
	return s.listener.Close()
}

type call struct {
	Method string `json:"method"`
	Path   string `json:"path"`
}

type listener struct {
	ID   string `json:"id"`
	Port int    `json:"port"`
}

type profile struct {
	Name string `json:"name"`
}

// localControlPlane is the shared, mutex-guarded receiver behind most of the
// admin endpoints.
type localControlPlane struct {
	mu        sync.Mutex
	calls     []call
	listeners map[string]listener
	profiles  map[string]profile
}

// ClearCalls drops the recorded call log and reports how many were dropped.
func (p *localControlPlane) ClearCalls() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	n := len(p.calls)
	p.calls = nil
	return n
}

// DeleteListener removes a listener by id.
func (p *localControlPlane) DeleteListener(id string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	if id == "" {
		return fmt.Errorf("listener id must not be empty")
	}
	if _, ok := p.listeners[id]; !ok {
		return fmt.Errorf("listener %q not found", id)
	}
	delete(p.listeners, id)
	return nil
}

// DeleteProfile removes a profile by name.
func (p *localControlPlane) DeleteProfile(name string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	if name == "" {
		return fmt.Errorf("profile name must not be empty")
	}
	if _, ok := p.profiles[name]; !ok {
		return fmt.Errorf("profile %q not found", name)
	}
	delete(p.profiles, name)
	return nil
}

// ListCalls returns a copy of the recorded call log.
func (p *localControlPlane) ListCalls() []call {
	p.mu.Lock()
	defer p.mu.Unlock()
	out := make([]call, len(p.calls))
	copy(out, p.calls)
	return out
}

// ListListeners returns the registered listener ids.
func (p *localControlPlane) ListListeners() []string {
	p.mu.Lock()
	defer p.mu.Unlock()
	out := make([]string, 0, len(p.listeners))
	for id := range p.listeners {
		out = append(out, id)
	}
	return out
}

// ListProfiles returns the registered profile names.
func (p *localControlPlane) ListProfiles() []string {
	p.mu.Lock()
	defer p.mu.Unlock()
	out := make([]string, 0, len(p.profiles))
	for name := range p.profiles {
		out = append(out, name)
	}
	return out
}

// localBaseURL builds the loopback base URL for a bound port.
func localBaseURL(port int) string {
	if port <= 0 || port > 65535 {
		return ""
	}
	return fmt.Sprintf("http://127.0.0.1:%d", port)
}

// writeJSON encodes v as a JSON response body.
func writeJSON(status int, v any) (string, error) {
	if status < 100 || status > 599 {
		return "", fmt.Errorf("invalid status %d", status)
	}
	body, err := json.Marshal(v)
	if err != nil {
		return "", err
	}
	return string(body), nil
}

// localResourceName normalizes an admin resource path segment.
func localResourceName(path string) string {
	trimmed := strings.Trim(path, "/")
	if trimmed == "" {
		return "root"
	}
	parts := strings.Split(trimmed, "/")
	return strings.ToLower(parts[0])
}
