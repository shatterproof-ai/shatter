package frontendsetup

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// runFrontend drives a real protocol.Handler with the production planner
// registration (RegisterDefaultPlanner) over a scripted request sequence,
// returning every response. This exercises the same wiring the shipped
// frontend binary uses, rather than a hand-built lookup closure.
func runFrontend(t *testing.T, requests ...string) []protocol.Response {
	t.Helper()
	input := strings.NewReader(strings.Join(requests, "\n") + "\n")
	var output bytes.Buffer
	handler := protocol.NewHandler(input, &output, io.Discard)
	RegisterDefaultPlanner(handler)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	var responses []protocol.Response
	for _, line := range strings.Split(strings.TrimSpace(output.String()), "\n") {
		if line == "" {
			continue
		}
		var resp protocol.Response
		if err := json.Unmarshal([]byte(line), &resp); err != nil {
			t.Fatalf("unmarshal response: %v (raw: %s)", err, line)
		}
		responses = append(responses, resp)
	}
	return responses
}

func frontendReq(id int, command string, extra ...string) string {
	base := fmt.Sprintf(`{"protocol_version":%q,"id":%d,"command":%q`, protocol.ProtocolVersion, id, command)
	for _, e := range extra {
		base += "," + e
	}
	return base + "}"
}

// TestConfiguredDefault_AppliedForFreeFunctionViaGetInvocationPlan is the
// str-79t9 layer-3 regression: a `defaults.<param>` entry configured for a FREE
// FUNCTION must reach the planner on the get_invocation_plan path and emit a
// top-priority literal ValuePlan carrying the configured value.
//
// This drives the full runtime chain the CLI uses — analyze populates the
// analysis cache, get_invocation_plan resolves the target through
// buildTargetContext, and hintConfigResolver loads .shatter/config.yaml from
// the target's SourceFile. The unit-level resolver tests pass a synthetic
// lookup closure and so cannot catch a break anywhere in that chain.
func TestConfiguredDefault_AppliedForFreeFunctionViaGetInvocationPlan(t *testing.T) {
	dir := t.TempDir()
	shatterDir := filepath.Join(dir, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatalf("mkdir .shatter: %v", err)
	}
	const configuredDir = "/fixtures/sample-anthropic"
	cfg := fmt.Sprintf(`
functions:
  "loader.go:loadOne":
    defaults:
      dir: %q
`, configuredDir)
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), []byte(cfg), 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}

	srcDir := filepath.Join(dir, "internal", "fixture")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatalf("mkdir src: %v", err)
	}
	src := "package fixture\n\n" +
		"func loadOne(dir string) int {\n" +
		"\tif dir == \"\" {\n\t\treturn 0\n\t}\n" +
		"\treturn len(dir)\n}\n"
	srcFile := filepath.Join(srcDir, "loader.go")
	if err := os.WriteFile(srcFile, []byte(src), 0o644); err != nil {
		t.Fatalf("write src: %v", err)
	}

	responses := runFrontend(t,
		frontendReq(1, "analyze", fmt.Sprintf(`"file":%q`, srcFile)),
		frontendReq(2, "get_invocation_plan",
			`"invocation_requirements":[{"target_id":"fixture:loadOne"}]`),
	)
	if len(responses) != 2 {
		t.Fatalf("responses len = %d, want 2 (%+v)", len(responses), responses)
	}
	if responses[0].Status != "analyze" {
		t.Fatalf("analyze status = %q (message: %s)", responses[0].Status, responses[0].Message)
	}
	plan := responses[1]
	if plan.Status != "invocation_plan" {
		t.Fatalf("plan status = %q (message: %s)", plan.Status, plan.Message)
	}
	if len(plan.InvocationPlans) == 0 {
		t.Fatalf("no invocation plans returned (unsatisfied: %+v)", plan.UnsatisfiedRequirements)
	}

	wantLiteral := fmt.Sprintf("%q", configuredDir)
	var got []string
	for _, p := range plan.InvocationPlans {
		for _, ap := range p.ArgumentPlans {
			if ap.ParamName != "dir" {
				continue
			}
			got = append(got, fmt.Sprintf("%s/%s", ap.Kind, string(ap.Literal)))
			if ap.Kind == protocol.ValuePlanKindLiteral && string(ap.Literal) == wantLiteral {
				return
			}
		}
	}
	t.Fatalf("configured default %s never appeared as a literal ValuePlan for param %q; saw %v",
		wantLiteral, "dir", got)
}
