package mockidentity

import (
	// Both imports resolve to a package declared `package auth`; they can only
	// coexist in one file via aliases. A source-spelling mock key ("auth.GetName")
	// cannot tell them apart — only the resolved import path can.
	autha "mockidentity/a/auth"
	authb "mockidentity/b/auth"
)

// UseA calls a/auth.GetName through the alias `autha`.
func UseA() string { return autha.GetName() }

// UseB calls b/auth.GetName through the alias `authb`.
func UseB() string { return authb.GetName() }
