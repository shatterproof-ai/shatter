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
