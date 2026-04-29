package protocol

import (
	"os"
	"path/filepath"
	"testing"

	"pgregory.net/rapid"
)

// Property: hash is deterministic — recomputing on identical inputs yields the
// same hash. Generates a random Go file body, writes it to a fresh tmp
// package, and asserts ComputeDiscoveryHash returns the same string twice.
func TestProperty_DiscoveryHashDeterministic(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		body := genGoFileBody().Draw(rt, "body")
		filter := rapid.StringMatching(`|[A-Z][a-zA-Z0-9_]{0,8}`).Draw(rt, "filter")

		dir := t.TempDir()
		writeGoModForProperty(t, dir)
		target := filepath.Join(dir, "f.go")
		if err := os.WriteFile(target, []byte(body), 0o644); err != nil {
			rt.Fatalf("write target: %v", err)
		}

		first, err := ComputeDiscoveryHash(target, filter)
		if err != nil {
			rt.Fatalf("first hash: %v", err)
		}
		second, err := ComputeDiscoveryHash(target, filter)
		if err != nil {
			rt.Fatalf("second hash: %v", err)
		}
		if first != second {
			rt.Errorf("hash not deterministic: %q vs %q (filter=%q)", first, second, filter)
		}
	})
}

// Property: hash is sensitive — flipping a single byte in the target file
// produces a different hash. Picks a random byte position and either toggles
// case or replaces with an unrelated literal byte; either way the file
// content changes and the hash must too.
func TestProperty_DiscoveryHashSourceSensitive(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		body := genGoFileBody().Draw(rt, "body")
		// Skip degenerate case: empty body cannot be mutated.
		if len(body) == 0 {
			return
		}

		dir := t.TempDir()
		writeGoModForProperty(t, dir)
		target := filepath.Join(dir, "f.go")
		if err := os.WriteFile(target, []byte(body), 0o644); err != nil {
			rt.Fatalf("write target: %v", err)
		}

		before, err := ComputeDiscoveryHash(target, "")
		if err != nil {
			rt.Fatalf("hash before: %v", err)
		}

		mutated := []byte(body)
		idx := rapid.IntRange(0, len(mutated)-1).Draw(rt, "idx")
		mutated[idx] ^= 0x20 // flip a bit; almost always changes byte
		if mutated[idx] == byte(body[idx]) {
			// Skip: the bit we flipped happened to be in a 0x20 boundary
			// and produced the same byte (impossible with XOR 0x20 on a
			// non-zero byte, but guard anyway).
			return
		}
		if err := os.WriteFile(target, mutated, 0o644); err != nil {
			rt.Fatalf("rewrite target: %v", err)
		}
		after, err := ComputeDiscoveryHash(target, "")
		if err != nil {
			rt.Fatalf("hash after: %v", err)
		}
		if before == after {
			rt.Errorf("hash failed to invalidate on byte flip at idx=%d (body=%q)", idx, body)
		}
	})
}

// Property: hash is order-independent — the order in which sibling files are
// CREATED on disk must not affect the hash. The implementation reads files
// in sorted-path order, so creating b.go before a.go must produce the same
// hash as creating a.go before b.go.
//
// Note: hashes inherently depend on the absolute directory path (it is a
// deliberate cache-key input so different packages can't cross-contaminate).
// To isolate the creation-order signal, both trials use the SAME directory:
// write in one order, hash, delete, write in the other order, hash.
func TestProperty_DiscoveryHashOrderIndependent(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		fileA := "package p\n\nfunc A() int { return 1 }\n"
		fileB := "package p\n\nfunc B() int { return 2 }\n"

		dir := t.TempDir()
		writeGoModForProperty(t, dir)
		pathA := filepath.Join(dir, "a.go")
		pathB := filepath.Join(dir, "b.go")

		// First trial: write a.go before b.go.
		if err := os.WriteFile(pathA, []byte(fileA), 0o644); err != nil {
			rt.Fatalf("trial1 write a: %v", err)
		}
		if err := os.WriteFile(pathB, []byte(fileB), 0o644); err != nil {
			rt.Fatalf("trial1 write b: %v", err)
		}
		hash1, err := ComputeDiscoveryHash(pathA, "")
		if err != nil {
			rt.Fatalf("trial1 hash: %v", err)
		}

		// Clean directory and re-create in reverse order. Same dir,
		// same path→content mapping, only creation order differs.
		if err := os.Remove(pathA); err != nil {
			rt.Fatalf("remove a: %v", err)
		}
		if err := os.Remove(pathB); err != nil {
			rt.Fatalf("remove b: %v", err)
		}
		if err := os.WriteFile(pathB, []byte(fileB), 0o644); err != nil {
			rt.Fatalf("trial2 write b: %v", err)
		}
		if err := os.WriteFile(pathA, []byte(fileA), 0o644); err != nil {
			rt.Fatalf("trial2 write a: %v", err)
		}
		hash2, err := ComputeDiscoveryHash(pathA, "")
		if err != nil {
			rt.Fatalf("trial2 hash: %v", err)
		}

		if hash1 != hash2 {
			rt.Errorf("hash depends on filesystem creation order: %q vs %q", hash1, hash2)
		}
	})
}

func writeGoModForProperty(t *testing.T, dir string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module example.com/p\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
}

// genGoFileBody emits a tiny but compilable Go source body with one or two
// trivial functions. We intentionally generate small files so the hashing
// path executes in negligible time; the property under test is byte-level,
// not language-semantic.
func genGoFileBody() *rapid.Generator[string] {
	return rapid.Custom(func(rt *rapid.T) string {
		fnCount := rapid.IntRange(1, 2).Draw(rt, "fnCount")
		body := "package p\n\n"
		for i := 0; i < fnCount; i++ {
			name := rapid.StringMatching(`[A-Z][a-zA-Z0-9_]{0,5}`).Draw(rt, "fnName")
			constant := rapid.IntRange(0, 100).Draw(rt, "constant")
			body += "func " + name + "() int { return " + itoa(constant) + " }\n"
		}
		return body
	})
}

func itoa(n int) string {
	// Avoid pulling strconv into the hot path of a generator.
	if n == 0 {
		return "0"
	}
	digits := []byte{}
	for n > 0 {
		digits = append([]byte{byte('0' + n%10)}, digits...)
		n /= 10
	}
	return string(digits)
}
