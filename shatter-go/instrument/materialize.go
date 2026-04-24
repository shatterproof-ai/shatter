package instrument

import (
	"fmt"
	"os"

	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
)

// MaterializeInstrumentedDirectory writes the instrumented source, recorder,
// and go.mod/go.sum support files into outputDir. The caller is responsible for
// choosing and cleaning up outputDir.
func MaterializeInstrumentedDirectory(
	sourcePath string,
	funcName *string,
	outputDir string,
	projectRoot *string,
	timing *frontendtiming.Collector,
) error {
	if outputDir == "" {
		return fmt.Errorf("instrument: MaterializeInstrumentedDirectory: outputDir must not be empty")
	}
	if err := os.MkdirAll(outputDir, 0o755); err != nil {
		return fmt.Errorf("instrument: create output dir: %w", err)
	}
	return InstrumentFileToDir(sourcePath, outputDir, funcName, projectRoot, timing)
}
