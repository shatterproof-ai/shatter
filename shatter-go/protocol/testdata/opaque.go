package testdata

import (
	"database/sql"
	"io"
	"net"
	"net/http"
	"os"
)

// AcceptsChanInt takes a channel of ints.
func AcceptsChanInt(ch chan int) int {
	return <-ch
}

// AcceptsChanString takes a channel of strings.
func AcceptsChanString(ch chan string) string {
	return <-ch
}

// AcceptsNetConn takes a net.Conn.
func AcceptsNetConn(conn net.Conn) string {
	return conn.RemoteAddr().String()
}

// AcceptsOsFile takes an *os.File.
func AcceptsOsFile(f *os.File) string {
	return f.Name()
}

// AcceptsIOReader takes an io.Reader.
func AcceptsIOReader(r io.Reader) int {
	buf := make([]byte, 1)
	n, _ := r.Read(buf)
	return n
}

// AcceptsIOWriter takes an io.Writer.
func AcceptsIOWriter(w io.Writer) int {
	n, _ := w.Write([]byte("hello"))
	return n
}

// AcceptsSqlDB takes a *sql.DB.
func AcceptsSqlDB(db *sql.DB) error {
	return db.Ping()
}

// AcceptsSqlTx takes a *sql.Tx.
func AcceptsSqlTx(tx *sql.Tx) error {
	return tx.Commit()
}

// AcceptsResponseWriter takes an http.ResponseWriter.
func AcceptsResponseWriter(w http.ResponseWriter) {
	w.WriteHeader(200)
}

// AcceptsNetListener takes a net.Listener.
func AcceptsNetListener(ln net.Listener) error {
	return ln.Close()
}

// AcceptsPlainInterface takes a plain interface (not opaque).
func AcceptsPlainInterface(v interface{}) string {
	if v == nil {
		return "nil"
	}
	return "non-nil"
}
