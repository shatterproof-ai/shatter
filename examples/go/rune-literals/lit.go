package lit

// Classify is the audit probe for str-49drv.102: an escaped string literal, a
// quote-containing string literal and a rune literal compared against params.
func Classify(s string, c rune) int {
	if s == "a\tb" {
		return 1
	}
	if s == "'q'" {
		return 2
	}
	if c == 'x' {
		return 3
	}
	return 0
}

// QuoteString compares against a string literal that contains quote characters.
func QuoteString(s string) string {
	if s == "'q'" {
		return "quoted"
	}
	return "other"
}

// RuneCompare compares a rune parameter against a rune literal.
func RuneCompare(c rune) string {
	if c == 'x' {
		return "x"
	}
	return "other"
}

// ByteCompare compares a byte parameter against a rune literal.
func ByteCompare(b byte) string {
	if b == 'x' {
		return "x"
	}
	return "other"
}

const sepRune = 'y'

// CountRune ranges over a string param comparing against rune literals; the
// literal pool must seed the string param with "x" (and "y" from the const).
func CountRune(s string) int {
	n := 0
	for _, c := range s {
		if c == 'x' || c == sepRune {
			n++
		}
	}
	return n
}
