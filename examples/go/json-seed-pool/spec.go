// Package jsonseedpool is the str-b27zm E2E fixture for config-driven seed
// pools on a structured-decode parameter.
//
// ClassifySpec decodes data via encoding/json into a Spec struct and
// branches on the decoded content. Neither random byte mutation nor a
// single fixed literal can drive this function's decode-success branches:
// almost no random byte string is valid JSON at all (everything lands on
// "invalid"), and a lone literal default only ever reaches one branch. The
// fixture's `.shatter/config.yaml` supplies a `seeds` pool of example
// documents for the `data` parameter — one per branch — so the explorer's
// candidate/mutation source has several valid document shapes to start
// from and vary around instead of pure-random bytes (mirrors Zolem's
// specs/openapi.go NormalizeOpenAPI shape: a []byte param json.Unmarshal'd
// into a struct, branching on the decoded fields).
package jsonseedpool

import "encoding/json"

// Spec is the decode target. Only OpenAPI and Info.Title drive branching;
// everything else is ignored, mirroring a real normalizer that only reads a
// handful of top-level fields from an otherwise large document.
type Spec struct {
	OpenAPI string `json:"openapi"`
	Info    struct {
		Title string `json:"title"`
	} `json:"info"`
}

// ClassifySpec buckets data by its decoded OpenAPI version field:
//
//   - malformed JSON                         -> "invalid"
//   - openapi == ""                          -> "missing-version"
//   - openapi == "2.0"                       -> "swagger2"
//   - openapi starts with "3."                -> "openapi3"
//   - any other version, with a title         -> "unknown-version"
//   - any other version, without a title      -> "untitled"
func ClassifySpec(data []byte) string {
	var spec Spec
	if err := json.Unmarshal(data, &spec); err != nil {
		return "invalid"
	}
	switch {
	case spec.OpenAPI == "":
		return "missing-version"
	case spec.OpenAPI == "2.0":
		return "swagger2"
	case len(spec.OpenAPI) >= 2 && spec.OpenAPI[:2] == "3.":
		return "openapi3"
	case spec.Info.Title != "":
		return "unknown-version"
	default:
		return "untitled"
	}
}
