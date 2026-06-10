package testdata

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"os"
	"text/template"
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

// MarshalPlainInterface exercises the JSON-encoding empty-interface pattern
// used by helper functions such as zolem's writeJSONObject.
func MarshalPlainInterface(v interface{}) ([]byte, error) {
	return json.Marshal(v)
}

// EncodePlainInterface exercises json.NewEncoder(...).Encode(v) for empty
// interface parameters.
func EncodePlainInterface(w io.Writer, v any) error {
	return json.NewEncoder(w).Encode(v)
}

// DecodePlainInterface is intentionally out of scope for the first
// interface-candidate slice; decode destinations require pointer synthesis.
func DecodePlainInterface(r *http.Request, v any) error {
	return json.NewDecoder(r.Body).Decode(v)
}

// AcceptsRequestPointer takes a *http.Request. The Body field is an
// io.ReadCloser; pre-str-gxjs the analyzer flagged this whole tree as
// opaque (Body → io.ReadCloser → I/O stream) and skipped the function.
// Post-str-gxjs *http.Request short-circuits to a synthesizable kind.
func AcceptsRequestPointer(r *http.Request) string {
	return r.Method
}

var errInert = errors.New("inert transport performs no I/O")

type inertTransport struct{}

func (inertTransport) RoundTrip(*http.Request) (*http.Response, error) {
	return nil, errInert
}

// AcceptsIOReadCloser takes an io.ReadCloser — historically opaque,
// post-str-gxjs synthesizable via io.NopCloser(strings.NewReader("")).
func AcceptsIOReadCloser(rc io.ReadCloser) error {
	return rc.Close()
}

// AcceptsContext takes a context.Context, which the runtime-value registry can
// synthesize with context.Background().
func AcceptsContext(ctx context.Context) error {
	return ctx.Err()
}

// AcceptsTemplatePointer takes a parsed text/template value. The analyzer
// should treat the template pointer as a synthesizable runtime value instead
// of descending into text/template/parse.Node internals.
func AcceptsTemplatePointer(t *template.Template) error {
	return t.Execute(io.Discard, nil)
}

// TemplateHolder stores a template pointer behind a struct field, matching
// fixture-like values that own parsed templates internally.
type TemplateHolder struct {
	Name     string
	Template *template.Template
}

// AcceptsTemplateHolder exercises composite synthesis for structs that carry a
// template field. The field may be nil, but planning must not require
// parse.Node construction.
func AcceptsTemplateHolder(h TemplateHolder) bool {
	return h.Template != nil || h.Name != ""
}
