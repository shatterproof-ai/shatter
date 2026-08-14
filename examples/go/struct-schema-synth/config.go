// Package structschemasynth is the str-4q7bd E2E fixture for schema-aware
// structured-input synthesis: no `.shatter/config.yaml` seed pool exists
// here (contrast with the str-b27zm sibling fixture `json-seed-pool/`,
// which relies entirely on operator-configured seeds). ClassifyConfig
// decodes data via encoding/json into a nested Config struct and branches
// on the decoded content. Reaching any branch past "invalid" requires a
// structurally valid JSON *object* matching Config's shape — pure random
// byte mutation essentially never produces valid JSON, so without
// schema-aware synthesis these branches are unreachable without an
// operator-supplied seed. This mirrors Zolem's specs/discovery.go
// NormalizeGeminiDiscovery shape (a []byte param json.Unmarshal'd into a
// struct with a nested object field).
package structschemasynth

import "encoding/json"

// Limits is a nested decode target, exercising synthesis of a struct field
// whose own type is a named struct (not just a primitive).
type Limits struct {
	Max int `json:"max"`
}

// Config is the decode target for ClassifyConfig.
type Config struct {
	Name    string   `json:"name"`
	Enabled bool     `json:"enabled"`
	Limits  Limits   `json:"limits"`
	Tags    []string `json:"tags"`
}

// ClassifyConfig buckets data by its decoded content:
//
//   - malformed/empty JSON        -> "invalid"
//   - name == ""                  -> "missing-name"
//   - !enabled                    -> "disabled"
//   - limits.max <= 0             -> "no-limit"
//   - len(tags) == 0              -> "no-tags"
//   - otherwise                   -> "configured"
func ClassifyConfig(data []byte) string {
	var cfg Config
	if err := json.Unmarshal(data, &cfg); err != nil {
		return "invalid"
	}
	switch {
	case cfg.Name == "":
		return "missing-name"
	case !cfg.Enabled:
		return "disabled"
	case cfg.Limits.Max <= 0:
		return "no-limit"
	case len(cfg.Tags) == 0:
		return "no-tags"
	default:
		return "configured"
	}
}
