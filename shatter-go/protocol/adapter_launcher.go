package protocol

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"go/ast"
	"path/filepath"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	goloader "github.com/shatter-dev/shatter/shatter-go/loader"
	"github.com/shatter-dev/shatter/shatter-go/sandbox"
)

func executeAdapterViaLauncher(adapterID string, ctx InvocationContext) (*instrument.ExecuteResult, error) {
	program, err := prepareAdapterLauncher(ctx.File, ctx.FunctionName, adapterID)
	if err != nil {
		return nil, err
	}
	defer program.Cleanup()

	return program.Invoke(ctx.Inputs, ctx.Capture)
}

func prepareAdapterLauncher(file, function, adapterID string) (*preparedLauncher, error) {
	absoluteFilePath, err := filepath.Abs(file)
	if err != nil {
		return nil, fmt.Errorf("normalize file path: %w", err)
	}

	ws, err := resolveExecutionWorkspace(absoluteFilePath)
	if err != nil {
		return nil, fmt.Errorf("initialize workspace: %w", err)
	}
	if err := ws.Ensure(); err != nil {
		return nil, fmt.Errorf("ensure workspace: %w", err)
	}

	ldr, err := goloader.New(ws)
	if err != nil {
		return nil, fmt.Errorf("construct analyzer loader: %w", err)
	}
	pkg, err := loadPackageForAnalysis(ldr, absoluteFilePath)
	if err != nil {
		return nil, fmt.Errorf("load package: %w", err)
	}

	packageDir, err := packageDirForBuild(pkg)
	if err != nil {
		return nil, err
	}
	modulePath, moduleDir, err := moduleInfoForBuild(pkg, packageDir)
	if err != nil {
		return nil, err
	}

	discoveryHash := adapterDiscoveryHash(adapterID, absoluteFilePath, function)
	if pkg.Name == "main" && !ast.IsExported(function) {
		return nil, fmt.Errorf("unexported package main HTTP handler %q cannot be invoked through import-based adapter launcher", function)
	}
	// Instrument the target package (recorder + runtime support + exported
	// adapter bridge) so the httptest-driven invocation records branch_path /
	// lines_executed just like the normal wrapper launcher path (str-1qd5i).
	overlayPath, runtimeDir, err := build.BuildAdapterInstrumentationOverlay(packageDir, pkg.Name, discoveryHash, ws.GeneratedDir())
	if err != nil {
		return nil, err
	}
	mainSource, err := generateAdapterLauncherMain(adapterID, packageImportPathForBuild(pkg, modulePath), function)
	if err != nil {
		return nil, err
	}

	binaryPath, _, err := launcher.BuildLauncher(launcher.BuildOptions{
		TargetModulePath:  modulePath,
		TargetModuleDir:   moduleDir,
		TargetImportPath:  packageImportPathForBuild(pkg, modulePath),
		DiscoveryHash:     discoveryHash,
		GeneratedDir:      ws.GeneratedDir(),
		BinariesDir:       ws.BinariesDir(),
		GoEnv:             ws.GoEnv(),
		OverlayPath:       overlayPath,
		MainSource:        mainSource,
		UseHarnessLoop:    true,
		HarnessRuntimeDir: runtimeDir,
	})
	if err != nil {
		return nil, fmt.Errorf("build adapter launcher: %w", err)
	}

	return &preparedLauncher{
		BinaryPath:  binaryPath,
		ProjectRoot: moduleDir,
		WorkDir:     moduleDir,
		Sandbox:     sandbox.FromEnv(),
		// Adapter-owned launcher exposes a synthetic invocation surface
		// rather than a wrapper-target-keyed switch; TargetID and the
		// receiver_kind override are unused on the adapter path. Leave
		// them blank — the adapter's launcher main_source generates its
		// own dispatch and ignores PlanDescriptor entirely.
	}, nil
}

func adapterDiscoveryHash(adapterID, file, function string) string {
	h := sha256.New()
	fmt.Fprintf(h, "%s\x00%s\x00%s\x00", adapterID, file, function)
	return hex.EncodeToString(h.Sum(nil))[:16]
}

func generateAdapterLauncherMain(adapterID, targetImportPath, function string) (string, error) {
	if isReceiverQualifiedFunctionName(function) {
		return "", fmt.Errorf("adapter launcher does not support receiver method target %q", function)
	}
	switch adapterID {
	case HTTPHandlerAdapterID:
		return generateHTTPAdapterLauncherMain(targetImportPath, function), nil
	case GinAdapterID:
		return generateGinAdapterLauncherMain(targetImportPath, function), nil
	default:
		return "", fmt.Errorf("unsupported adapter launcher: %s", adapterID)
	}
}

// writeAdapterRecorderDrain appends, to an adapter launcher main, the code that
// reads the instrumented target's recorder (branch_path / lines_executed /
// scope_events) after the handler returns and copies each non-empty field into
// resp. It runs inside the harness.RunLoop closure at two-tab indentation and
// leaves resp's pre-initialized empty arrays untouched on any drain or decode
// error, so a recorder failure degrades to empty instrumentation rather than a
// failed invocation (str-1qd5i).
func writeAdapterRecorderDrain(b *strings.Builder) {
	b.WriteString("\t\tif _rec, _recErr := target.ShatterAdapterResults(); _recErr == nil {\n")
	b.WriteString("\t\t\tvar _results struct {\n")
	b.WriteString("\t\t\t\tLinesExecuted json.RawMessage `json:\"lines_executed\"`\n")
	b.WriteString("\t\t\t\tBranchPath    json.RawMessage `json:\"branch_path\"`\n")
	b.WriteString("\t\t\t\tScopeEvents   json.RawMessage `json:\"scope_events\"`\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tif _uErr := json.Unmarshal(_rec, &_results); _uErr == nil {\n")
	b.WriteString("\t\t\t\tif len(_results.BranchPath) > 0 {\n")
	b.WriteString("\t\t\t\t\tresp.BranchPath = _results.BranchPath\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t\tif len(_results.LinesExecuted) > 0 {\n")
	b.WriteString("\t\t\t\t\tresp.LinesExecuted = _results.LinesExecuted\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t\tif len(_results.ScopeEvents) > 0 {\n")
	b.WriteString("\t\t\t\t\tresp.ScopeEvents = _results.ScopeEvents\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
}

func generateHTTPAdapterLauncherMain(targetImportPath, function string) string {
	var b strings.Builder
	b.WriteString("// Code generated by Shatter. DO NOT EDIT.\n")
	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"net/http\"\n")
	b.WriteString("\t\"net/http/httptest\"\n")
	b.WriteString("\t\"strings\"\n\n")
	b.WriteString("\t\"shatter-harness\"\n\n")
	fmt.Fprintf(&b, "\ttarget %q\n", targetImportPath)
	b.WriteString(")\n\n")
	b.WriteString("func main() {\n")
	b.WriteString("\tharness.RunLoop(func(req harness.Request) harness.Response {\n")
	b.WriteString("\t\tperf := harness.StartPerf()\n")
	b.WriteString("\t\tresp := harness.Response{\n")
	b.WriteString("\t\t\tBranchPath:    json.RawMessage(\"[]\"),\n")
	b.WriteString("\t\t\tLinesExecuted: json.RawMessage(\"[]\"),\n")
	b.WriteString("\t\t\tScopeEvents:   json.RawMessage(\"[]\"),\n")
	b.WriteString("\t\t\tSideEffects:   []harness.SideEffect{},\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif len(req.Inputs) != 4 {\n")
	b.WriteString("\t\t\tresp.Error = fmt.Sprintf(\"http handler adapter expects 4 inputs, got %d\", len(req.Inputs))\n")
	b.WriteString("\t\t\tresp.Performance = perf.Finish()\n")
	b.WriteString("\t\t\treturn resp\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tvar method string\n")
	b.WriteString("\t\tvar path string\n")
	b.WriteString("\t\tvar headers map[string]string\n")
	b.WriteString("\t\tvar body string\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[0], &method); err != nil { resp.Error = fmt.Sprintf(\"unmarshal method: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[1], &path); err != nil { resp.Error = fmt.Sprintf(\"unmarshal path: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[2], &headers); err != nil { resp.Error = fmt.Sprintf(\"unmarshal headers: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[3], &body); err != nil { resp.Error = fmt.Sprintf(\"unmarshal body: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\trecorder := httptest.NewRecorder()\n")
	b.WriteString("\t\thttpReq := httptest.NewRequest(method, path, strings.NewReader(body))\n")
	b.WriteString("\t\tfor k, v := range headers {\n")
	b.WriteString("\t\t\thttpReq.Header.Set(k, v)\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\ttarget.ShatterAdapterReset()\n")
	b.WriteString("\t\tthrown := harness.SafeCall(func() {\n")
	fmt.Fprintf(&b, "\t\t\thttp.HandlerFunc(target.%s).ServeHTTP(recorder, httpReq)\n", function)
	b.WriteString("\t\t})\n")
	b.WriteString("\t\tresp.ThrownError = thrown\n")
	writeAdapterRecorderDrain(&b)
	b.WriteString("\t\tresult := recorder.Result()\n")
	b.WriteString("\t\tdefer result.Body.Close()\n")
	b.WriteString("\t\tpayload, err := json.Marshal(struct {\n")
	b.WriteString("\t\t\tStatus  int                 `json:\"status\"`\n")
	b.WriteString("\t\t\tHeaders map[string][]string `json:\"headers\"`\n")
	b.WriteString("\t\t\tBody    string              `json:\"body\"`\n")
	b.WriteString("\t\t}{\n")
	b.WriteString("\t\t\tStatus:  result.StatusCode,\n")
	b.WriteString("\t\t\tHeaders: recorder.Header(),\n")
	b.WriteString("\t\t\tBody:    recorder.Body.String(),\n")
	b.WriteString("\t\t})\n")
	b.WriteString("\t\tif err != nil { resp.Error = fmt.Sprintf(\"marshal response: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tresp.ReturnValue = payload\n")
	b.WriteString("\t\tresp.Performance = perf.Finish()\n")
	b.WriteString("\t\treturn resp\n")
	b.WriteString("\t})\n")
	b.WriteString("}\n")
	return b.String()
}

func generateGinAdapterLauncherMain(targetImportPath, function string) string {
	var b strings.Builder
	b.WriteString("// Code generated by Shatter. DO NOT EDIT.\n")
	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"net/http/httptest\"\n")
	b.WriteString("\t\"strings\"\n\n")
	b.WriteString("\t\"github.com/gin-gonic/gin\"\n")
	b.WriteString("\t\"shatter-harness\"\n\n")
	fmt.Fprintf(&b, "\ttarget %q\n", targetImportPath)
	b.WriteString(")\n\n")
	b.WriteString("func main() {\n")
	b.WriteString("\tgin.SetMode(gin.TestMode)\n")
	b.WriteString("\tharness.RunLoop(func(req harness.Request) harness.Response {\n")
	b.WriteString("\t\tperf := harness.StartPerf()\n")
	b.WriteString("\t\tresp := harness.Response{\n")
	b.WriteString("\t\t\tBranchPath:    json.RawMessage(\"[]\"),\n")
	b.WriteString("\t\t\tLinesExecuted: json.RawMessage(\"[]\"),\n")
	b.WriteString("\t\t\tScopeEvents:   json.RawMessage(\"[]\"),\n")
	b.WriteString("\t\t\tSideEffects:   []harness.SideEffect{},\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif len(req.Inputs) != 5 {\n")
	b.WriteString("\t\t\tresp.Error = fmt.Sprintf(\"gin handler adapter expects 5 inputs, got %d\", len(req.Inputs))\n")
	b.WriteString("\t\t\tresp.Performance = perf.Finish()\n")
	b.WriteString("\t\t\treturn resp\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tvar method string\n")
	b.WriteString("\t\tvar path string\n")
	b.WriteString("\t\tvar headers map[string]string\n")
	b.WriteString("\t\tvar body string\n")
	b.WriteString("\t\tvar routeParams map[string]string\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[0], &method); err != nil { resp.Error = fmt.Sprintf(\"unmarshal method: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[1], &path); err != nil { resp.Error = fmt.Sprintf(\"unmarshal path: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[2], &headers); err != nil { resp.Error = fmt.Sprintf(\"unmarshal headers: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[3], &body); err != nil { resp.Error = fmt.Sprintf(\"unmarshal body: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tif err := json.Unmarshal(req.Inputs[4], &routeParams); err != nil { resp.Error = fmt.Sprintf(\"unmarshal route_params: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\trecorder := httptest.NewRecorder()\n")
	b.WriteString("\t\tctx, _ := gin.CreateTestContext(recorder)\n")
	b.WriteString("\t\thttpReq := httptest.NewRequest(method, path, strings.NewReader(body))\n")
	b.WriteString("\t\tfor k, v := range headers {\n")
	b.WriteString("\t\t\thttpReq.Header.Set(k, v)\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tctx.Request = httpReq\n")
	b.WriteString("\t\tif len(routeParams) > 0 {\n")
	b.WriteString("\t\t\tparams := make(gin.Params, 0, len(routeParams))\n")
	b.WriteString("\t\t\tfor k, v := range routeParams {\n")
	b.WriteString("\t\t\t\tparams = append(params, gin.Param{Key: k, Value: v})\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tctx.Params = params\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\ttarget.ShatterAdapterReset()\n")
	b.WriteString("\t\tthrown := harness.SafeCall(func() {\n")
	fmt.Fprintf(&b, "\t\t\ttarget.%s(ctx)\n", function)
	b.WriteString("\t\t})\n")
	b.WriteString("\t\tresp.ThrownError = thrown\n")
	writeAdapterRecorderDrain(&b)
	b.WriteString("\t\tresult := recorder.Result()\n")
	b.WriteString("\t\tdefer result.Body.Close()\n")
	b.WriteString("\t\tpayload, err := json.Marshal(struct {\n")
	b.WriteString("\t\t\tStatus  int                 `json:\"status\"`\n")
	b.WriteString("\t\t\tHeaders map[string][]string `json:\"headers\"`\n")
	b.WriteString("\t\t\tBody    string              `json:\"body\"`\n")
	b.WriteString("\t\t}{\n")
	b.WriteString("\t\t\tStatus:  result.StatusCode,\n")
	b.WriteString("\t\t\tHeaders: recorder.Header(),\n")
	b.WriteString("\t\t\tBody:    recorder.Body.String(),\n")
	b.WriteString("\t\t})\n")
	b.WriteString("\t\tif err != nil { resp.Error = fmt.Sprintf(\"marshal response: %v\", err); resp.Performance = perf.Finish(); return resp }\n")
	b.WriteString("\t\tresp.ReturnValue = payload\n")
	b.WriteString("\t\tresp.Performance = perf.Finish()\n")
	b.WriteString("\t\treturn resp\n")
	b.WriteString("\t})\n")
	b.WriteString("}\n")
	return b.String()
}
