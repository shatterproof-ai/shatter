package pathconstructor

import "os"

type Recorder struct {
	path string
}

func NewRecorder(path string) *Recorder {
	return &Recorder{path: path}
}

func (r *Recorder) Record(event string) string {
	if r == nil {
		return "nil"
	}
	if r.path == "" {
		return "missing-path"
	}
	if event == "" {
		return "empty"
	}
	if err := os.WriteFile(r.path, []byte(event), 0o600); err != nil {
		return "write-error"
	}
	return "recorded"
}
