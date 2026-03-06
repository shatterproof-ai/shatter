package main

import (
	"fmt"
	"math"
	"strings"
)

// Example 5: Interface-based dispatch (Go equivalent of discriminated unions).
// Tests shatter's ability to enumerate type variants via interface + type switch.

// Shape is implemented by Circle, RectangleShape, and Triangle.
type Shape interface {
	Kind() string
}

type Circle struct{ Radius float64 }
type RectangleShape struct{ Width, Height float64 }
type Triangle struct{ Base, Height float64 }

func (c Circle) Kind() string         { return "circle" }
func (r RectangleShape) Kind() string  { return "rectangle" }
func (t Triangle) Kind() string        { return "triangle" }

// ComputeArea — 6 branches: circle+radius≤0→error, circle→πr²,
// rectangle+dim≤0→error, rectangle→w*h, triangle+dim≤0→error, triangle→0.5*b*h.
// Analyzer should detect type switch arms and nested dimension validation.
func ComputeArea(shape Shape) (float64, error) {
	switch s := shape.(type) {
	case Circle:
		if s.Radius <= 0 {
			return 0, fmt.Errorf("non-positive radius")
		}
		return math.Pi * s.Radius * s.Radius, nil
	case RectangleShape:
		if s.Width <= 0 || s.Height <= 0 {
			return 0, fmt.Errorf("non-positive dimension")
		}
		return s.Width * s.Height, nil
	case Triangle:
		if s.Base <= 0 || s.Height <= 0 {
			return 0, fmt.Errorf("non-positive dimension")
		}
		return 0.5 * s.Base * s.Height, nil
	default:
		return 0, fmt.Errorf("unknown shape")
	}
}

// ApiRequest represents an HTTP API request with optional body.
type ApiRequest struct {
	Method        string
	Path          string
	Body          string
	HasBody       bool
	Authenticated bool
}

// RouteRequest — 8 branches: empty path→error, GET→"read",
// DELETE+!auth→error, DELETE+auth→"delete", POST+no body→error, POST→"create",
// PUT+no body→error, PUT→"update".
// Analyzer should detect string-equality dispatch on Method with nested checks.
func RouteRequest(req ApiRequest) (string, error) {
	if len(req.Path) == 0 {
		return "", fmt.Errorf("empty path")
	}

	switch strings.ToUpper(req.Method) {
	case "GET":
		return "read", nil
	case "DELETE":
		if !req.Authenticated {
			return "", fmt.Errorf("auth required")
		}
		return "delete", nil
	case "POST":
		if !req.HasBody {
			return "", fmt.Errorf("body required")
		}
		return "create", nil
	case "PUT":
		if !req.HasBody {
			return "", fmt.Errorf("body required")
		}
		return "update", nil
	default:
		return "", fmt.Errorf("unknown method")
	}
}
