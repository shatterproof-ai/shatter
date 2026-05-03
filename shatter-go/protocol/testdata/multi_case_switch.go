package testdata

// DetectLanguageID is the str-qo1.11 regression fixture: a multi-case switch
// with a default clause. Pre-fix, the analyzer enumerated branches per case
// literal and silently dropped the default clause, while the instrumentor
// recorded a branch_id for every CaseClause (default included). The denominator
// (analyzer count) was therefore one less than the numerator (instrumentor
// IDs covered), producing a 110%-style "11/10 branches" report on focused
// exploration. Each case + default must count as one branch obligation; the
// fixture is intentionally exhaustive so 100%-coverage exploration cannot
// exceed the denominator.
func DetectLanguageID(ext string) string {
	switch ext {
	case ".go":
		return "go"
	case ".ts":
		return "typescript"
	case ".tsx":
		return "typescript"
	case ".js":
		return "javascript"
	case ".jsx":
		return "javascript"
	case ".py":
		return "python"
	case ".rs":
		return "rust"
	case ".rb":
		return "ruby"
	case ".java":
		return "java"
	case ".cpp":
		return "cpp"
	default:
		return "unknown"
	}
}
