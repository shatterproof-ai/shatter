// Example 11: External dependencies with third-party packages
// Tests shatter's handling of functions that import real third-party modules.
// Unlike 06-external-deps.go which only uses stdlib, this file imports
// packages that require `go mod tidy` / `go mod download` to resolve.
//
// Uses:
//   - github.com/go-chi/chi/v5       — lightweight HTTP router
//   - github.com/go-playground/validator/v10 — struct/field validation
//   - github.com/tidwall/gjson       — JSON path queries

package examples

import (
	"fmt"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-playground/validator/v10"
	"github.com/tidwall/gjson"
)

// BuildRouter — 4 branches based on HTTP method.
// Exercises chi's router API: NewRouter(), Get/Post/Delete route registration.
//
// EXPECTED BRANCHES (4):
//   1. method == "GET"    → "get:/path"
//   2. method == "POST"   → "post:/path"
//   3. method == "DELETE" → "delete:/path"
//   4. default            → "unsupported"
func BuildRouter(method string, path string) string {
	r := chi.NewRouter()

	switch method {
	case "GET":
		r.Get(path, func(w http.ResponseWriter, r *http.Request) {})
		return fmt.Sprintf("get:%s", path)
	case "POST":
		r.Post(path, func(w http.ResponseWriter, r *http.Request) {})
		return fmt.Sprintf("post:%s", path)
	case "DELETE":
		r.Delete(path, func(w http.ResponseWriter, r *http.Request) {})
		return fmt.Sprintf("delete:%s", path)
	default:
		return "unsupported"
	}
}

// ValidateUser — 4 branches based on struct validation results.
// Exercises go-playground/validator's Struct() method with tag-based rules.
// The validator actually inspects the struct fields against the tags.
//
// EXPECTED BRANCHES (4):
//   1. name is empty       → "invalid:Name"
//   2. email is malformed  → "invalid:Email"
//   3. age is out of range → "invalid:Age"
//   4. all fields valid    → "valid"
func ValidateUser(name string, email string, age int) string {
	type User struct {
		Name  string `validate:"required"`
		Email string `validate:"required,email"`
		Age   int    `validate:"gte=0,lte=150"`
	}

	v := validator.New()
	u := User{Name: name, Email: email, Age: age}
	err := v.Struct(u)
	if err == nil {
		return "valid"
	}

	errs := err.(validator.ValidationErrors)
	return fmt.Sprintf("invalid:%s", errs[0].Field())
}

// ExtractJsonField — 5 branches based on gjson path query results.
// Exercises gjson's Get() to query JSON by dotted path, then classifies
// the result type using gjson.Result methods.
//
// EXPECTED BRANCHES (5):
//   1. json is empty           → "error:empty"
//   2. path not found          → "missing"
//   3. value is a string       → "string:<value>"
//   4. value is a number       → "number:<value>"
//   5. value is another type   → "other"
func ExtractJsonField(json string, path string) string {
	if json == "" {
		return "error:empty"
	}

	result := gjson.Get(json, path)
	if !result.Exists() {
		return "missing"
	}

	switch result.Type {
	case gjson.String:
		return fmt.Sprintf("string:%s", result.Str)
	case gjson.Number:
		return fmt.Sprintf("number:%g", result.Num)
	default:
		return "other"
	}
}
