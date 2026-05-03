// NoTargetReason::ParserFailure — file with intentionally malformed syntax.
// The TS frontend's parser should fail and the orchestrator should classify
// the file as `parser_failure` rather than aborting the run.
export function broken(input: number): number {
  if (input >
}
