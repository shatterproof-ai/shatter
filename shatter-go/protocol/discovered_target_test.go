package protocol

import (
	"encoding/json"
	"go/ast"
	"go/token"
	"go/types"
	"path/filepath"
	"runtime"
	"testing"

	"pgregory.net/rapid"
)

// targetsFixture holds the loaded test package for testdata/targets.go.
type targetsFixture struct {
	fset *token.FileSet
	file *ast.File
	info *types.Info
}

func loadTargetsFixture(t *testing.T) targetsFixture {
	t.Helper()
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)

	_, callerFile, _, _ := runtime.Caller(0)
	absPath, err := filepath.Abs(filepath.Join(filepath.Dir(callerFile), "testdata", "targets.go"))
	if err != nil {
		t.Fatalf("abs path: %v", err)
	}

	pkg, err := loadPackageForAnalysis(ldr, absPath)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}

	file := findTargetSyntaxFile(pkg, absPath)
	if file == nil {
		t.Fatalf("targets.go not found in package syntax")
	}

	return targetsFixture{fset: pkg.Fset, file: file, info: pkg.TypesInfo}
}

func findFuncDeclByName(file *ast.File, name string) *ast.FuncDecl {
	for _, decl := range file.Decls {
		if fn, ok := decl.(*ast.FuncDecl); ok && fn.Name.Name == name {
			return fn
		}
	}
	return nil
}

func TestBuildDiscoveredTargetKinds(t *testing.T) {
	fix := loadTargetsFixture(t)

	cases := []struct {
		funcName     string
		wantKind     TargetKind
		wantRecv     *ReceiverShape
		wantQualName string
		wantVis      string
	}{
		{
			funcName:     "Increment",
			wantKind:     TargetKindFunction,
			wantRecv:     nil,
			wantQualName: "Increment",
			wantVis:      "exported",
		},
		{
			funcName:     "hidden",
			wantKind:     TargetKindFunction,
			wantRecv:     nil,
			wantQualName: "hidden",
			wantVis:      "unexported",
		},
		{
			funcName:     "Add",
			wantKind:     TargetKindMethod,
			wantRecv:     &ReceiverShape{TypeName: "Counter", IsPointer: false},
			wantQualName: "(Counter).Add",
			wantVis:      "exported",
		},
		{
			funcName:     "Reset",
			wantKind:     TargetKindMethod,
			wantRecv:     &ReceiverShape{TypeName: "Counter", IsPointer: true},
			wantQualName: "(*Counter).Reset",
			wantVis:      "exported",
		},
	}

	for _, tc := range cases {
		t.Run(tc.funcName, func(t *testing.T) {
			fn := findFuncDeclByName(fix.file, tc.funcName)
			if fn == nil {
				t.Fatalf("function %q not found in testdata/targets.go", tc.funcName)
			}

			filePath := fix.fset.Position(fn.Pos()).Filename
			target := BuildDiscoveredTarget(fix.fset, fn, fix.info, "example.com/pkg", "pkg", filePath)

			if target.Kind != tc.wantKind {
				t.Errorf("Kind = %q, want %q", target.Kind, tc.wantKind)
			}
			if target.QualifiedName != tc.wantQualName {
				t.Errorf("QualifiedName = %q, want %q", target.QualifiedName, tc.wantQualName)
			}
			if target.SymbolName != tc.funcName {
				t.Errorf("SymbolName = %q, want %q", target.SymbolName, tc.funcName)
			}
			if target.Visibility != tc.wantVis {
				t.Errorf("Visibility = %q, want %q", target.Visibility, tc.wantVis)
			}
			if tc.wantRecv == nil {
				if target.Receiver != nil {
					t.Errorf("Receiver = %+v, want nil", target.Receiver)
				}
			} else {
				if target.Receiver == nil {
					t.Errorf("Receiver = nil, want %+v", tc.wantRecv)
				} else {
					if target.Receiver.TypeName != tc.wantRecv.TypeName {
						t.Errorf("Receiver.TypeName = %q, want %q", target.Receiver.TypeName, tc.wantRecv.TypeName)
					}
					if target.Receiver.IsPointer != tc.wantRecv.IsPointer {
						t.Errorf("Receiver.IsPointer = %v, want %v", target.Receiver.IsPointer, tc.wantRecv.IsPointer)
					}
				}
			}
		})
	}
}

func TestBuildDiscoveredTargetIDStability(t *testing.T) {
	fix := loadTargetsFixture(t)

	fn := findFuncDeclByName(fix.file, "Increment")
	if fn == nil {
		t.Fatal("Increment not found in testdata/targets.go")
	}

	filePath := fix.fset.Position(fn.Pos()).Filename
	const pkgPath = "example.com/pkg"

	first := BuildDiscoveredTarget(fix.fset, fn, fix.info, pkgPath, "pkg", filePath)
	second := BuildDiscoveredTarget(fix.fset, fn, fix.info, pkgPath, "pkg", filePath)

	if first.ID != second.ID {
		t.Errorf("ID not stable across calls: %q != %q", first.ID, second.ID)
	}
	if want := pkgPath + ":Increment"; first.ID != want {
		t.Errorf("ID = %q, want %q", first.ID, want)
	}
}

func TestDiscoveredTargetJSONRoundtrip(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		target := genDiscoveredTarget().Draw(rt, "target")
		data, err := json.Marshal(target)
		if err != nil {
			rt.Fatalf("marshal: %v", err)
		}
		var got DiscoveredTarget
		if err := json.Unmarshal(data, &got); err != nil {
			rt.Fatalf("unmarshal: %v", err)
		}
		if got.ID != target.ID {
			rt.Errorf("ID: got %q, want %q", got.ID, target.ID)
		}
		if got.Kind != target.Kind {
			rt.Errorf("Kind: got %q, want %q", got.Kind, target.Kind)
		}
		if got.QualifiedName != target.QualifiedName {
			rt.Errorf("QualifiedName: got %q, want %q", got.QualifiedName, target.QualifiedName)
		}
		if got.Visibility != target.Visibility {
			rt.Errorf("Visibility: got %q, want %q", got.Visibility, target.Visibility)
		}
		if (got.Receiver == nil) != (target.Receiver == nil) {
			rt.Errorf("Receiver nil mismatch: got %v, want %v", got.Receiver, target.Receiver)
		}
		if target.Receiver != nil && got.Receiver != nil {
			if got.Receiver.TypeName != target.Receiver.TypeName {
				rt.Errorf("Receiver.TypeName: got %q, want %q", got.Receiver.TypeName, target.Receiver.TypeName)
			}
			if got.Receiver.IsPointer != target.Receiver.IsPointer {
				rt.Errorf("Receiver.IsPointer: got %v, want %v", got.Receiver.IsPointer, target.Receiver.IsPointer)
			}
			if got.Receiver.IsInterface != target.Receiver.IsInterface {
				rt.Errorf("Receiver.IsInterface: got %v, want %v", got.Receiver.IsInterface, target.Receiver.IsInterface)
			}
		}
		if got.HasTypeParams != target.HasTypeParams {
			rt.Errorf("HasTypeParams: got %v, want %v", got.HasTypeParams, target.HasTypeParams)
		}
		if got.HasCGoDep != target.HasCGoDep {
			rt.Errorf("HasCGoDep: got %v, want %v", got.HasCGoDep, target.HasCGoDep)
		}
		if got.IsTestFile != target.IsTestFile {
			rt.Errorf("IsTestFile: got %v, want %v", got.IsTestFile, target.IsTestFile)
		}
	})
}

func TestBuildDiscoveredTargetClassificationFields(t *testing.T) {
	fix := loadTargetsFixture(t)

	t.Run("non_generic_function_has_no_type_params", func(t *testing.T) {
		fn := findFuncDeclByName(fix.file, "Increment")
		if fn == nil {
			t.Fatal("Increment not found")
		}
		target := BuildDiscoveredTarget(fix.fset, fn, fix.info, "example.com/pkg", "pkg", fix.fset.Position(fn.Pos()).Filename)
		if target.HasTypeParams {
			t.Errorf("HasTypeParams = true for non-generic function, want false")
		}
	})

	t.Run("generic_function_has_type_params", func(t *testing.T) {
		fn := findFuncDeclByName(fix.file, "Map")
		if fn == nil {
			t.Fatal("Map not found in testdata/targets.go")
		}
		target := BuildDiscoveredTarget(fix.fset, fn, fix.info, "example.com/pkg", "pkg", fix.fset.Position(fn.Pos()).Filename)
		if !target.HasTypeParams {
			t.Errorf("HasTypeParams = false for generic function, want true")
		}
	})

	t.Run("non_test_file_has_is_test_file_false", func(t *testing.T) {
		fn := findFuncDeclByName(fix.file, "Increment")
		if fn == nil {
			t.Fatal("Increment not found")
		}
		target := BuildDiscoveredTarget(fix.fset, fn, fix.info, "example.com/pkg", "pkg", "/src/pkg/foo.go")
		if target.IsTestFile {
			t.Errorf("IsTestFile = true for non-test path, want false")
		}
	})

	t.Run("test_file_path_sets_is_test_file", func(t *testing.T) {
		fn := findFuncDeclByName(fix.file, "Increment")
		if fn == nil {
			t.Fatal("Increment not found")
		}
		target := BuildDiscoveredTarget(fix.fset, fn, fix.info, "example.com/pkg", "pkg", "/src/pkg/foo_test.go")
		if !target.IsTestFile {
			t.Errorf("IsTestFile = false for _test.go path, want true")
		}
	})

	t.Run("concrete_method_receiver_is_not_interface", func(t *testing.T) {
		fn := findFuncDeclByName(fix.file, "Reset")
		if fn == nil {
			t.Fatal("Reset not found")
		}
		target := BuildDiscoveredTarget(fix.fset, fn, fix.info, "example.com/pkg", "pkg", fix.fset.Position(fn.Pos()).Filename)
		if target.Receiver == nil {
			t.Fatal("Receiver is nil")
		}
		if target.Receiver.IsInterface {
			t.Errorf("Receiver.IsInterface = true for concrete type receiver, want false")
		}
	})
}

func genReceiverShape() *rapid.Generator[*ReceiverShape] {
	return rapid.OneOf(
		rapid.Just[*ReceiverShape](nil),
		rapid.Custom(func(rt *rapid.T) *ReceiverShape {
			return &ReceiverShape{
				TypeName:    rapid.StringMatching(`[A-Z][a-zA-Z0-9]{1,8}`).Draw(rt, "type_name"),
				IsPointer:   rapid.Bool().Draw(rt, "is_pointer"),
				IsInterface: rapid.Bool().Draw(rt, "is_interface"),
			}
		}),
	)
}

func genDiscoveredTarget() *rapid.Generator[DiscoveredTarget] {
	return rapid.Custom(func(rt *rapid.T) DiscoveredTarget {
		pkgPath := "example.com/" + rapid.StringMatching(`[a-z]{3,8}`).Draw(rt, "pkg")
		qualName := rapid.StringMatching(`[A-Za-z][a-zA-Z0-9]{1,10}`).Draw(rt, "qual_name")
		kind := rapid.SampledFrom([]TargetKind{TargetKindFunction, TargetKindMethod, TargetKindAdapter}).Draw(rt, "kind")
		vis := rapid.SampledFrom([]string{"exported", "unexported"}).Draw(rt, "visibility")
		return DiscoveredTarget{
			ID:            pkgPath + ":" + qualName,
			PackagePath:   pkgPath,
			PackageName:   rapid.StringMatching(`[a-z]{3,8}`).Draw(rt, "pkg_name"),
			FilePath:      "/src/" + rapid.StringMatching(`[a-z]{3,8}`).Draw(rt, "file") + ".go",
			StartLine:     rapid.IntRange(1, 500).Draw(rt, "start"),
			EndLine:       rapid.IntRange(1, 500).Draw(rt, "end"),
			SymbolName:    rapid.StringMatching(`[A-Za-z][a-zA-Z0-9]{1,10}`).Draw(rt, "sym"),
			QualifiedName: qualName,
			Kind:          kind,
			Receiver:      genReceiverShape().Draw(rt, "receiver"),
			Parameters:    []ParamInfo{},
			Results:       []TypeInfo{},
			Visibility:    vis,
			HasTypeParams: rapid.Bool().Draw(rt, "has_type_params"),
			HasCGoDep:     rapid.Bool().Draw(rt, "has_cgo_dep"),
			IsTestFile:    rapid.Bool().Draw(rt, "is_test_file"),
		}
	})
}
