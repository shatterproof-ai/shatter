package usergens

import (
	"encoding/json"
	"io"
	"os"
	"testing"
)

func TestFileHandleFreshGeneration(t *testing.T) {
	result := FileHandleWithKind(ContentSmallText, ModeFile)
	if result.Value == nil {
		t.Fatal("expected non-nil Value")
	}
	f, ok := result.Value.(*os.File)
	if !ok {
		t.Fatalf("expected *os.File, got %T", result.Value)
	}
	defer f.Close()
	defer cleanupRecipe(t, result.Recipe)

	info, err := f.Stat()
	if err != nil {
		t.Fatalf("stat failed: %v", err)
	}
	if info.Size() == 0 {
		t.Error("expected non-empty file for small_text content")
	}
	if result.ID != "file-handle" {
		t.Errorf("ID = %q, want %q", result.ID, "file-handle")
	}
}

func TestFileHandleReplay(t *testing.T) {
	original := FileHandleWithKind(ContentSmallText, ModeFile)
	if original.Value == nil {
		t.Fatal("fresh generation returned nil Value")
	}
	origFile := original.Value.(*os.File)
	origContent, err := io.ReadAll(origFile)
	if err != nil {
		t.Fatalf("read original: %v", err)
	}
	origFile.Close()

	// Replay from recipe.
	replayed := FileHandle(original.Recipe)
	if replayed.Value == nil {
		t.Fatal("replay returned nil Value")
	}
	defer cleanupRecipe(t, original.Recipe)

	replayFile, ok := replayed.Value.(*os.File)
	if !ok {
		t.Fatalf("replay expected *os.File, got %T", replayed.Value)
	}
	defer replayFile.Close()

	replayContent, err := io.ReadAll(replayFile)
	if err != nil {
		t.Fatalf("read replay: %v", err)
	}
	if string(replayContent) != string(origContent) {
		t.Errorf("replay content mismatch: got %d bytes, want %d bytes", len(replayContent), len(origContent))
	}
}

func TestFileHandleReaderMode(t *testing.T) {
	result := FileHandleWithKind(ContentSmallText, ModeReader)
	if result.Value == nil {
		t.Fatal("expected non-nil Value")
	}
	defer cleanupRecipe(t, result.Recipe)

	r, ok := result.Value.(io.Reader)
	if !ok {
		t.Fatalf("expected io.Reader, got %T", result.Value)
	}

	// Should NOT be *os.File directly.
	if _, isFile := result.Value.(*os.File); isFile {
		t.Error("reader mode should not return *os.File directly")
	}

	data, err := io.ReadAll(r)
	if err != nil {
		t.Fatalf("read failed: %v", err)
	}
	if len(data) == 0 {
		t.Error("expected non-empty read from small_text content")
	}
}

func TestFileHandleReadCloserMode(t *testing.T) {
	result := FileHandleWithKind(ContentSmallText, ModeReadCloser)
	if result.Value == nil {
		t.Fatal("expected non-nil Value")
	}
	defer cleanupRecipe(t, result.Recipe)

	rc, ok := result.Value.(io.ReadCloser)
	if !ok {
		t.Fatalf("expected io.ReadCloser, got %T", result.Value)
	}

	// Should NOT be *os.File directly.
	if _, isFile := result.Value.(*os.File); isFile {
		t.Error("read_closer mode should not return *os.File directly")
	}

	data, err := io.ReadAll(rc)
	if err != nil {
		t.Fatalf("read failed: %v", err)
	}
	if len(data) == 0 {
		t.Error("expected non-empty read from small_text content")
	}
	if err := rc.Close(); err != nil {
		t.Errorf("close failed: %v", err)
	}
}

func TestFileHandleContentVariants(t *testing.T) {
	tests := []struct {
		kind     string
		wantSize int
	}{
		{ContentEmpty, 0},
		{ContentSmallText, 45},
		{ContentBinary, 256},
		{ContentLarge, 65536},
	}

	for _, tt := range tests {
		t.Run(tt.kind, func(t *testing.T) {
			result := FileHandleWithKind(tt.kind, ModeFile)
			if result.Value == nil {
				t.Fatal("expected non-nil Value")
			}
			f := result.Value.(*os.File)
			defer f.Close()
			defer cleanupRecipe(t, result.Recipe)

			data, err := io.ReadAll(f)
			if err != nil {
				t.Fatalf("read failed: %v", err)
			}
			if len(data) != tt.wantSize {
				t.Errorf("content size = %d, want %d", len(data), tt.wantSize)
			}
		})
	}
}

func TestCleanupFileHandle(t *testing.T) {
	result := FileHandleWithKind(ContentSmallText, ModeFile)
	if result.Value == nil {
		t.Fatal("expected non-nil Value")
	}

	var rec FileRecipe
	if err := json.Unmarshal(result.Recipe, &rec); err != nil {
		t.Fatalf("unmarshal recipe: %v", err)
	}

	// Close the file before cleanup.
	result.Value.(*os.File).Close()

	// Verify file exists before cleanup.
	if _, err := os.Stat(rec.TempPath); err != nil {
		t.Fatalf("temp file should exist before cleanup: %v", err)
	}

	if err := CleanupFileHandle(result.Recipe); err != nil {
		t.Fatalf("cleanup failed: %v", err)
	}

	// Verify file is removed.
	if _, err := os.Stat(rec.TempPath); !os.IsNotExist(err) {
		t.Error("temp file should be removed after cleanup")
	}
}

func TestCleanupNilRecipe(t *testing.T) {
	if err := CleanupFileHandle(nil); err != nil {
		t.Errorf("cleanup with nil recipe should not error: %v", err)
	}
}

func TestFileHandleRandomFresh(t *testing.T) {
	// Calling FileHandle with nil recipe should produce a valid result.
	result := FileHandle(nil)
	if result.Value == nil {
		t.Fatal("expected non-nil Value from random generation")
	}
	defer cleanupRecipe(t, result.Recipe)

	if result.ID != "file-handle" {
		t.Errorf("ID = %q, want %q", result.ID, "file-handle")
	}
	if result.Recipe == nil {
		t.Error("expected non-nil recipe")
	}
}

// cleanupRecipe is a test helper that removes the temp file from a recipe.
func cleanupRecipe(t *testing.T, recipe json.RawMessage) {
	t.Helper()
	if err := CleanupFileHandle(recipe); err != nil {
		t.Errorf("cleanup failed: %v", err)
	}
}
