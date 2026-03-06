package main

import "strings"

// Example 10: HTTP path routing and middleware resolution.
// Tests reasoning about string patterns, slice lookups, and multi-condition dispatch.

// Route defines a route pattern with method and handler name.
type Route struct {
	Method  string
	Pattern string
	Handler string
}

// MatchResult holds the outcome of a route match attempt.
type MatchResult struct {
	Status  string
	Handler string
	Params  map[string]string
	Message string
}

// MatchRoute — 12 branches: empty path→error, no leading /→error,
// empty method→error, no routes→not-found, exact static match→matched,
// param match (:id)→matched+params, wildcard (*)→matched+wildcard,
// method mismatch→method-not-allowed, no match→not-found,
// first match wins, trailing slash normalized, multiple params extracted.
func MatchRoute(routes []Route, method string, path string) MatchResult {
	if len(path) == 0 {
		return MatchResult{Status: "error", Message: "empty path"}
	}
	if !strings.HasPrefix(path, "/") {
		return MatchResult{Status: "error", Message: "path must start with /"}
	}
	if len(method) == 0 {
		return MatchResult{Status: "error", Message: "empty method"}
	}
	if len(routes) == 0 {
		return MatchResult{Status: "not-found"}
	}

	normalizedPath := path
	if len(path) > 1 && strings.HasSuffix(path, "/") {
		normalizedPath = path[:len(path)-1]
	}

	pathSegments := splitSegments(normalizedPath)
	upperMethod := strings.ToUpper(method)
	pathMatchedButMethodDiffers := false

	for _, route := range routes {
		patternSegments := splitSegments(route.Pattern)

		if len(patternSegments) > 0 && patternSegments[len(patternSegments)-1] == "*" {
			prefixSegments := patternSegments[:len(patternSegments)-1]
			if len(pathSegments) >= len(prefixSegments) {
				prefixMatches := true
				params := make(map[string]string)

				for i := 0; i < len(prefixSegments); i++ {
					ps := prefixSegments[i]
					if strings.HasPrefix(ps, ":") {
						params[ps[1:]] = pathSegments[i]
					} else if ps != pathSegments[i] {
						prefixMatches = false
						break
					}
				}

				if prefixMatches {
					if strings.ToUpper(route.Method) == upperMethod {
						params["wildcard"] = strings.Join(pathSegments[len(prefixSegments):], "/")
						return MatchResult{Status: "matched", Handler: route.Handler, Params: params}
					}
					pathMatchedButMethodDiffers = true
				}
			}
			continue
		}

		if len(patternSegments) != len(pathSegments) {
			continue
		}

		matches := true
		params := make(map[string]string)
		for i := 0; i < len(patternSegments); i++ {
			ps := patternSegments[i]
			if strings.HasPrefix(ps, ":") {
				params[ps[1:]] = pathSegments[i]
			} else if ps != pathSegments[i] {
				matches = false
				break
			}
		}

		if matches {
			if strings.ToUpper(route.Method) == upperMethod {
				return MatchResult{Status: "matched", Handler: route.Handler, Params: params}
			}
			pathMatchedButMethodDiffers = true
		}
	}

	if pathMatchedButMethodDiffers {
		return MatchResult{Status: "method-not-allowed"}
	}
	return MatchResult{Status: "not-found"}
}

func splitSegments(path string) []string {
	var segments []string
	for _, s := range strings.Split(path, "/") {
		if len(s) > 0 {
			segments = append(segments, s)
		}
	}
	return segments
}

// RouteMetadata describes middleware requirements for a route.
type RouteMetadata struct {
	RequiresAuth   bool
	ContentType    string
	CorsOrigin     string
	HasCorsOrigin  bool
	AllowedOrigins []string
}

// MiddlewareChain holds the resolved middleware list.
type MiddlewareChain struct {
	Status     string
	Middleware []string
	Reason     string
}

// ResolveMiddleware — 8 branches: auth required+no header→rejected,
// auth+invalid format→rejected, auth+valid→adds auth middleware,
// json content→adds json-parser, multipart→adds multipart-parser,
// cors+allowed origin→adds cors, cors+disallowed→rejected, base chain only.
func ResolveMiddleware(metadata RouteMetadata, authHeader string) MiddlewareChain {
	chain := []string{"logging", "error-handler"}

	if metadata.RequiresAuth {
		if len(authHeader) == 0 {
			return MiddlewareChain{Status: "rejected", Middleware: nil, Reason: "missing auth"}
		}
		if !strings.HasPrefix(authHeader, "Bearer ") || len(authHeader) <= 7 {
			return MiddlewareChain{Status: "rejected", Middleware: nil, Reason: "invalid auth format"}
		}
		chain = append(chain, "auth")
	}

	if metadata.ContentType == "application/json" {
		chain = append(chain, "json-parser")
	} else if metadata.ContentType == "multipart/form-data" {
		chain = append(chain, "multipart-parser")
	}

	if metadata.HasCorsOrigin {
		found := false
		for _, origin := range metadata.AllowedOrigins {
			if origin == metadata.CorsOrigin {
				found = true
				break
			}
		}
		if found {
			chain = append(chain, "cors")
		} else {
			return MiddlewareChain{Status: "rejected", Middleware: nil, Reason: "origin not allowed"}
		}
	}

	return MiddlewareChain{Status: "ok", Middleware: chain}
}
