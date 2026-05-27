package launcher

// Exported for testing only.

var ReadTargetGoMod = readTargetGoMod

func InternalAnchorRelForTest(modulePath, importPath string) string {
	rel, err := internalAnchorRel(modulePath, importPath)
	if err != nil {
		return "ERR:" + err.Error()
	}
	return rel
}

// str-bni0 test hooks.

func SweepOrphanedLauncherDirsForTest(launchersParent string) {
	sweepOrphanedLauncherDirs(launchersParent)
}

func RegisterActiveLauncherDirForTest(dir string) {
	registerActiveLauncherDir(dir)
}

func UnregisterActiveLauncherDirForTest(dir string) {
	unregisterActiveLauncherDir(dir)
}

func HasActiveLauncherDirsForTest() bool {
	return hasActiveLauncherDirs()
}

func LaunchersDirNameForTest() string {
	return launchersDirName
}
