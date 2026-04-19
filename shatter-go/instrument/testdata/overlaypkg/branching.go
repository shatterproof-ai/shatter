package overlaypkg

func Classify(x int) string {
	if x > 0 {
		return tag("pos")
	}
	return tag("nonpos")
}
