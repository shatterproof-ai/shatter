package testdata

import "fmt"

// SelectExample uses a select statement with channel operations.
func SelectExample(ch1 chan int, ch2 chan string) {
	select {
	case v := <-ch1:
		fmt.Println(v)
	case ch2 <- "hello":
		fmt.Println("sent")
	default:
		fmt.Println("no activity")
	}
}

// SelectNoDefault uses a select without a default case.
func SelectNoDefault(ch1 chan int, ch2 chan int) {
	select {
	case v := <-ch1:
		fmt.Println(v)
	case v := <-ch2:
		fmt.Println(v)
	}
}
