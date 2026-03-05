package protocol

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"strings"
	"sync"
)

// LevelTrace is a custom slog level below Debug, matching SHATTER_LOG_LEVEL=trace.
const LevelTrace = slog.LevelDebug - 4

func slogLevelFromEnv() slog.Level {
	return slogLevelFromString(os.Getenv("SHATTER_LOG_LEVEL"))
}

func slogLevelFromString(s string) slog.Level {
	switch strings.ToLower(s) {
	case "error":
		return slog.LevelError
	case "warn":
		return slog.LevelWarn
	case "debug":
		return slog.LevelDebug
	case "trace":
		return LevelTrace
	default:
		return slog.LevelInfo
	}
}

// prefixHandler is a minimal slog.Handler that writes "[shatter-go] msg" lines,
// preserving the existing log format expected by the core engine and tests.
type prefixHandler struct {
	w     io.Writer
	mu    *sync.Mutex
	level slog.Level
}

func newPrefixHandler(w io.Writer, level slog.Level) *prefixHandler {
	return &prefixHandler{w: w, mu: &sync.Mutex{}, level: level}
}

func (h *prefixHandler) Enabled(_ context.Context, level slog.Level) bool {
	return level >= h.level
}

func (h *prefixHandler) Handle(_ context.Context, r slog.Record) error {
	// Build "[shatter-go] msg key=val key=val\n"
	var buf strings.Builder
	buf.WriteString("[shatter-go] ")
	buf.WriteString(r.Message)
	r.Attrs(func(a slog.Attr) bool {
		fmt.Fprintf(&buf, " %s=%v", a.Key, a.Value)
		return true
	})
	buf.WriteByte('\n')

	h.mu.Lock()
	defer h.mu.Unlock()
	_, err := io.WriteString(h.w, buf.String())
	return err
}

func (h *prefixHandler) WithAttrs(_ []slog.Attr) slog.Handler { return h }
func (h *prefixHandler) WithGroup(_ string) slog.Handler      { return h }
