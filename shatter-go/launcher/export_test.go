package launcher

import "golang.org/x/mod/modfile"

// Exported for testing only.

var ReadTargetGoMod = readTargetGoMod

func BuildLauncherGoModForTest(
	moduleName, targetModulePath, targetModuleDir string,
	useHarnessLoop bool,
	harnessRuntimeDir string,
	goVersion string,
	targetReplaces []*modfile.Replace,
) string {
	return buildLauncherGoMod(moduleName, targetModulePath, targetModuleDir, useHarnessLoop, harnessRuntimeDir, goVersion, targetReplaces)
}
