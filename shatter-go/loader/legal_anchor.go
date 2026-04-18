package loader

import (
	"fmt"
	"strings"

	"golang.org/x/tools/go/packages"
)

const (
	launcherPackagePrefix   = "shatter_launcher_"
	launcherCollisionFormat = "%s_%d"
	firstCollisionSuffix    = 2
)

// LegalAnchor returns the deepest legal import-path anchor for targetPkg.
// The anchor is the parent of the deepest "internal" segment, or the module
// root when the package path contains no "internal" segment.
//
// Vendored dependency handling is intentionally deferred.
func LegalAnchor(targetPkg *packages.Package) (string, error) {
	if targetPkg == nil {
		return "", fmt.Errorf("target package is required")
	}
	if targetPkg.Module == nil {
		return "", fmt.Errorf("target package %q has no module metadata", targetPkg.PkgPath)
	}

	modulePath := strings.TrimSpace(targetPkg.Module.Path)
	packagePath := strings.TrimSpace(targetPkg.PkgPath)
	if modulePath == "" {
		return "", fmt.Errorf("target package %q has an empty module path", packagePath)
	}
	if packagePath == "" {
		return "", fmt.Errorf("target package has an empty package path")
	}
	if !isWithinModule(modulePath, packagePath) {
		return "", fmt.Errorf(
			"target package %q crosses module boundary %q",
			packagePath,
			modulePath,
		)
	}

	segments := strings.Split(packagePath, "/")
	deepestInternalIndex := -1
	for index, segment := range segments {
		if segment == "internal" {
			deepestInternalIndex = index
		}
	}
	if deepestInternalIndex == -1 {
		return modulePath, nil
	}
	if deepestInternalIndex == 0 {
		return "", fmt.Errorf("target package %q has no legal anchor parent", packagePath)
	}
	return strings.Join(segments[:deepestInternalIndex], "/"), nil
}

// LauncherPackagePath returns the generated launcher package path rooted at the
// legal anchor for targetPkg. If packageExists reports that the candidate
// package path is already occupied, deterministic numeric suffixes are added
// until an unused package path is found.
func LauncherPackagePath(
	targetPkg *packages.Package,
	targetIDHash string,
	packageExists func(packagePath string) bool,
) (string, error) {
	anchorPath, err := LegalAnchor(targetPkg)
	if err != nil {
		return "", err
	}
	if strings.TrimSpace(targetIDHash) == "" {
		return "", fmt.Errorf("target ID hash is required")
	}

	basePackageName := launcherPackagePrefix + targetIDHash
	candidatePath := joinImportPath(anchorPath, basePackageName)
	if packageExists == nil || !packageExists(candidatePath) {
		return candidatePath, nil
	}

	for suffix := firstCollisionSuffix; ; suffix++ {
		candidateName := fmt.Sprintf(launcherCollisionFormat, basePackageName, suffix)
		candidatePath = joinImportPath(anchorPath, candidateName)
		if !packageExists(candidatePath) {
			return candidatePath, nil
		}
	}
}

func isWithinModule(modulePath string, packagePath string) bool {
	return packagePath == modulePath || strings.HasPrefix(packagePath, modulePath+"/")
}

func joinImportPath(prefix string, suffix string) string {
	if prefix == "" {
		return suffix
	}
	return prefix + "/" + suffix
}
