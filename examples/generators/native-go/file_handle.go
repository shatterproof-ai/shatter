package usergens

import (
	"encoding/json"
	"math/rand"
	"os"

	"github.com/shatter-dev/shatter/shatter-go/generators"
)

// Content kinds for generated temp files.
const (
	ContentEmpty     = "empty"
	ContentSmallText = "small_text"
	ContentBinary    = "binary"
	ContentLarge     = "large"
)

// Wrapping modes controlling the type of the returned Value.
const (
	ModeFile       = "file"        // *os.File
	ModeReader     = "reader"      // io.Reader
	ModeReadCloser = "read_closer" // io.ReadCloser
)

var contentKinds = []string{ContentEmpty, ContentSmallText, ContentBinary, ContentLarge}
var modes = []string{ModeFile, ModeReader, ModeReadCloser}

// FileRecipe is the serializable recipe for replaying a file handle generator.
type FileRecipe struct {
	TempPath    string `json:"tempPath"`
	ContentKind string `json:"contentKind"`
	Mode        string `json:"mode"`
}

// contentForKind returns the byte content for a given content kind.
func contentForKind(kind string) []byte {
	switch kind {
	case ContentEmpty:
		return nil
	case ContentSmallText:
		return []byte("The quick brown fox jumps over the lazy dog.\n")
	case ContentBinary:
		b := make([]byte, 256)
		for i := range b {
			b[i] = byte(i)
		}
		return b
	case ContentLarge:
		pattern := []byte("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\n")
		b := make([]byte, 0, 65536)
		for len(b) < 65536 {
			b = append(b, pattern...)
		}
		return b[:65536]
	default:
		return nil
	}
}

// FileHandle generates real *os.File handles (or io.Reader/io.ReadCloser wrappers)
// backed by temp files with varied content. On fresh generation (recipe == nil),
// it picks a random content kind and mode. On replay, it reopens from the stored path.
func FileHandle(recipe json.RawMessage) generators.NativeGeneratorResult {
	var rec FileRecipe

	if recipe != nil {
		_ = json.Unmarshal(recipe, &rec)
		// Replay: reopen existing temp file.
		f, err := os.Open(rec.TempPath)
		if err != nil {
			return generators.NativeGeneratorResult{
				ID:     "file-handle",
				Value:  nil,
				Recipe: recipe,
			}
		}
		recipeBytes, _ := json.Marshal(rec)
		return generators.NativeGeneratorResult{
			ID:     "file-handle",
			Value:  wrapFile(f, rec.Mode),
			Recipe: recipeBytes,
		}
	}

	// Fresh generation: random content kind and mode.
	rec.ContentKind = contentKinds[rand.Intn(len(contentKinds))]
	rec.Mode = modes[rand.Intn(len(modes))]

	return generateWithRecipe(rec)
}

// FileHandleWithKind generates a file handle with a specific content kind and mode.
// Useful for tests that need deterministic content.
func FileHandleWithKind(contentKind, mode string) generators.NativeGeneratorResult {
	return generateWithRecipe(FileRecipe{ContentKind: contentKind, Mode: mode})
}

func generateWithRecipe(rec FileRecipe) generators.NativeGeneratorResult {
	f, err := os.CreateTemp("", "shatter-gen-*")
	if err != nil {
		return generators.NativeGeneratorResult{ID: "file-handle", Value: nil}
	}

	content := contentForKind(rec.ContentKind)
	if len(content) > 0 {
		_, _ = f.Write(content)
		_, _ = f.Seek(0, 0)
	}

	rec.TempPath = f.Name()
	recipeBytes, _ := json.Marshal(rec)

	return generators.NativeGeneratorResult{
		ID:     "file-handle",
		Value:  wrapFile(f, rec.Mode),
		Recipe: recipeBytes,
	}
}

// wrapFile returns the *os.File as the appropriate interface type.
func wrapFile(f *os.File, mode string) any {
	switch mode {
	case ModeReader:
		return readerWrapper{f}
	case ModeReadCloser:
		return readCloserWrapper{f}
	default:
		return f
	}
}

// readerWrapper exposes only io.Reader, hiding *os.File's other methods.
type readerWrapper struct{ f *os.File }

func (r readerWrapper) Read(p []byte) (int, error) { return r.f.Read(p) }

// readCloserWrapper exposes only io.ReadCloser, hiding *os.File's other methods.
type readCloserWrapper struct{ f *os.File }

func (r readCloserWrapper) Read(p []byte) (int, error) { return r.f.Read(p) }
func (r readCloserWrapper) Close() error              { return r.f.Close() }

// CleanupFileHandle parses a recipe and removes the associated temp file.
// Call during teardown to avoid leaking temp files.
func CleanupFileHandle(recipe json.RawMessage) error {
	if recipe == nil {
		return nil
	}
	var rec FileRecipe
	if err := json.Unmarshal(recipe, &rec); err != nil {
		return err
	}
	if rec.TempPath == "" {
		return nil
	}
	// Best-effort close if still open — ignore errors since the handle
	// may already be closed or held by the caller.
	_ = os.Remove(rec.TempPath)
	return nil
}
