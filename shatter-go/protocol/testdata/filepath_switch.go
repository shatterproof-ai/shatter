package testdata

import "path/filepath"

// DetectServerKey switches on the filename extension to pick a language tag.
// Pure path/filepath helpers (filepath.Ext) must be classified as
// PureUtility by the auto-mock layer, otherwise filepath.Ext is mocked to
// return "" and only the default branch is reachable. See str-qo1.10.
func DetectServerKey(filePath string) string {
	switch filepath.Ext(filePath) {
	case ".go":
		return "go"
	case ".ts", ".tsx":
		return "typescript"
	case ".js", ".jsx":
		return "javascript"
	case ".py":
		return "python"
	case ".rs":
		return "rust"
	default:
		return "unknown"
	}
}

// JoinAndCheck exercises additional pure helpers: filepath.Join,
// filepath.Base, filepath.Dir, filepath.Clean, filepath.IsAbs.
func JoinAndCheck(dir, name string) string {
	full := filepath.Join(dir, name)
	cleaned := filepath.Clean(full)
	if filepath.IsAbs(cleaned) {
		return filepath.Base(cleaned)
	}
	return filepath.Dir(cleaned)
}
