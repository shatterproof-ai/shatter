package harnessembed

import (
	"os"
	"path/filepath"
	"sync"
	"testing"

	"pgregory.net/rapid"
)

// TestMirrorMatchesHarnessModule fails when the embedded mirror drifts from
// shatter-go/harness. Fix with `go generate ./harnessembed`.
func TestMirrorMatchesHarnessModule(t *testing.T) {
	contents, err := Contents()
	if err != nil {
		t.Fatal(err)
	}
	for name, mirrored := range contents {
		source, err := os.ReadFile(filepath.Join("..", "harness", name))
		if err != nil {
			t.Fatalf("read harness/%s: %v", name, err)
		}
		if string(source) != string(mirrored) {
			t.Errorf("harnessembed mirror of harness/%s is stale; run `go generate ./harnessembed`", name)
		}
	}
}

func TestMaterializeWritesModuleUnderHashDir(t *testing.T) {
	root := t.TempDir()
	dir, err := Materialize(root)
	if err != nil {
		t.Fatal(err)
	}
	hash, _ := Hash()
	if want := filepath.Join(root, "harness-runtime", hash); dir != want {
		t.Fatalf("dir = %q, want %q", dir, want)
	}
	contents, _ := Contents()
	for name, want := range contents {
		got, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil || string(got) != string(want) {
			t.Errorf("%s not materialized verbatim (err=%v)", name, err)
		}
	}
}

func TestMaterializeRepairsCorruptedDir(t *testing.T) {
	root := t.TempDir()
	dir, err := Materialize(root)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(dir, "runtime.go")); err != nil {
		t.Fatal(err)
	}
	again, err := Materialize(root)
	if err != nil || again != dir {
		t.Fatalf("re-materialize = %q, %v", again, err)
	}
}

func TestMaterializeRejectsEmptyRoot(t *testing.T) {
	if _, err := Materialize(""); err == nil {
		t.Fatal("expected error for empty root")
	}
}

func TestMaterializeUnwritableRootErrors(t *testing.T) {
	file := filepath.Join(t.TempDir(), "afile")
	if err := os.WriteFile(file, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Materialize(file); err == nil {
		t.Fatal("expected error when root is a regular file")
	}
}

func TestMaterializeConcurrentIsAtomicAndIdempotent(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		root := t.TempDir()
		n := rapid.IntRange(2, 8).Draw(rt, "callers")
		dirs := make([]string, n)
		errs := make([]error, n)
		var wg sync.WaitGroup
		for i := 0; i < n; i++ {
			wg.Add(1)
			go func(i int) {
				defer wg.Done()
				dirs[i], errs[i] = Materialize(root)
			}(i)
		}
		wg.Wait()
		contents, _ := Contents()
		for i := 0; i < n; i++ {
			if errs[i] != nil {
				rt.Fatalf("caller %d: %v", i, errs[i])
			}
			if dirs[i] != dirs[0] {
				rt.Fatalf("callers disagree: %q vs %q", dirs[i], dirs[0])
			}
			for name, want := range contents {
				got, err := os.ReadFile(filepath.Join(dirs[i], name))
				if err != nil || string(got) != string(want) {
					rt.Fatalf("%s incomplete for caller %d (err=%v)", name, i, err)
				}
			}
		}
	})
}
