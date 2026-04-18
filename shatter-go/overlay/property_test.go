package overlay

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"pgregory.net/rapid"
)

// genPathSegment generates a short, safe filename/directory component.
func genPathSegment() *rapid.Generator[string] {
	return rapid.StringMatching(`[a-z][a-z0-9_]{0,7}`)
}

type pair struct {
	inTreeRel string
	realRel   string
}

func genPairs() *rapid.Generator[[]pair] {
	return rapid.SliceOfNDistinct(
		rapid.Custom(func(t *rapid.T) pair {
			dirCount := rapid.IntRange(1, 3).Draw(t, "dirCount")
			segments := make([]string, dirCount)
			for i := range dirCount {
				segments[i] = genPathSegment().Draw(t, fmt.Sprintf("inTreeSeg%d", i))
			}
			inTree := filepath.Join(append(segments, genPathSegment().Draw(t, "inTreeFile")+".go")...)
			realRel := filepath.Join(
				genPathSegment().Draw(t, "realDir"),
				genPathSegment().Draw(t, "realFile")+".go",
			)
			return pair{inTreeRel: inTree, realRel: realRel}
		}),
		1, 6,
		func(p pair) string { return p.inTreeRel },
	)
}

func TestProperty_WriteReadRoundtrip(outer *testing.T) {
	rapid.Check(outer, func(t *rapid.T) {
		workspace, err := os.MkdirTemp("", "overlay-prop-*")
		if err != nil {
			t.Fatalf("mkdir temp: %v", err)
		}
		defer os.RemoveAll(workspace)
		pairs := genPairs().Draw(t, "pairs")

		// Ensure real rel paths are distinct too, else two in-tree paths could
		// collide on the same backing file — valid but noisy for this property.
		seenReal := make(map[string]struct{})
		uniq := pairs[:0]
		for _, p := range pairs {
			if _, ok := seenReal[p.realRel]; ok {
				continue
			}
			seenReal[p.realRel] = struct{}{}
			uniq = append(uniq, p)
		}
		pairs = uniq

		b := NewBuilder(filepath.Join(workspace, "overlays"), "plan")
		want := make(map[string]string, len(pairs))
		for _, p := range pairs {
			inAbs := filepath.Join(workspace, "mod", p.inTreeRel)
			realAbs := filepath.Join(workspace, "gen", p.realRel)
			if err := os.MkdirAll(filepath.Dir(realAbs), 0o755); err != nil {
				t.Fatalf("mkdir: %v", err)
			}
			if err := os.WriteFile(realAbs, []byte("package p\n"), 0o644); err != nil {
				t.Fatalf("write: %v", err)
			}
			if err := b.Add(inAbs, realAbs); err != nil {
				t.Fatalf("Add: %v", err)
			}
			want[inAbs] = realAbs
		}

		manifestPath, err := b.Write()
		if err != nil {
			t.Fatalf("Write: %v", err)
		}
		raw, err := os.ReadFile(manifestPath)
		if err != nil {
			t.Fatalf("read: %v", err)
		}
		var got Manifest
		if err := json.Unmarshal(raw, &got); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if len(got.Replace) != len(want) {
			t.Fatalf("length: got %d want %d", len(got.Replace), len(want))
		}
		for k, v := range want {
			if got.Replace[k] != v {
				t.Fatalf("mismatch for %q: got %q want %q", k, got.Replace[k], v)
			}
		}
	})
}

func TestProperty_AddIdempotent(outer *testing.T) {
	rapid.Check(outer, func(t *rapid.T) {
		workspace, err := os.MkdirTemp("", "overlay-idem-*")
		if err != nil {
			t.Fatalf("mkdir temp: %v", err)
		}
		defer os.RemoveAll(workspace)
		pairs := genPairs().Draw(t, "pairs")
		reps := rapid.IntRange(1, 4).Draw(t, "reps")

		b := NewBuilder(filepath.Join(workspace, "overlays"), "plan")
		for r := range reps {
			for _, p := range pairs {
				inAbs := filepath.Join(workspace, "mod", p.inTreeRel)
				realAbs := filepath.Join(workspace, "gen", p.realRel)
				if r == 0 {
					if err := os.MkdirAll(filepath.Dir(realAbs), 0o755); err != nil {
						t.Fatalf("mkdir: %v", err)
					}
					if err := os.WriteFile(realAbs, []byte("package p\n"), 0o644); err != nil {
						t.Fatalf("write: %v", err)
					}
				}
				if err := b.Add(inAbs, realAbs); err != nil {
					t.Fatalf("Add rep=%d: %v", r, err)
				}
			}
		}

		// Expected cardinality = distinct inTree paths after absolutization.
		expected := make(map[string]struct{})
		for _, p := range pairs {
			inAbs, _ := filepath.Abs(filepath.Join(workspace, "mod", p.inTreeRel))
			expected[inAbs] = struct{}{}
		}
		if len(b.replace) != len(expected) {
			t.Fatalf("replace length: got %d want %d", len(b.replace), len(expected))
		}
	})
}
