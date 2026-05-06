package workspace

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestVerifyMaterializedSource_NonEmptyFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "ok.go")
	if err := os.WriteFile(path, []byte("package x\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := VerifyMaterializedSource(path, true); err != nil {
		t.Fatalf("VerifyMaterializedSource(non-empty, true): %v", err)
	}
}

func TestVerifyMaterializedSource_EmptyFileWhenNonEmptyExpected(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "empty.go")
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	err := VerifyMaterializedSource(path, true)
	if err == nil {
		t.Fatal("expected error for zero-byte file when non-empty expected")
	}
	if !errors.Is(err, ErrEmptyMaterializedFile) {
		t.Fatalf("error %v does not wrap ErrEmptyMaterializedFile", err)
	}
}

func TestVerifyMaterializedSource_EmptyAllowedWhenNotExpected(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "empty.go")
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := VerifyMaterializedSource(path, false); err != nil {
		t.Fatalf("VerifyMaterializedSource(empty, false): %v", err)
	}
}

func TestVerifyMaterializedSource_MissingFile(t *testing.T) {
	dir := t.TempDir()
	missing := filepath.Join(dir, "nope.go")
	if err := VerifyMaterializedSource(missing, true); err == nil {
		t.Fatal("expected error for missing file")
	}
}

func TestVerifyMaterializedSource_DirectoryRejected(t *testing.T) {
	dir := t.TempDir()
	if err := VerifyMaterializedSource(dir, true); err == nil {
		t.Fatal("expected error when path is a directory")
	}
}
