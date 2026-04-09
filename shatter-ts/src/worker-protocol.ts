/**
 * Messages exchanged between the main thread and the instrumentation worker.
 *
 * All fields are structured-cloneable (no AST nodes, no class instances).
 * The worker handles CPU-bound analyze and instrument operations while the
 * main thread manages protocol I/O and mutable state.
 */

import type { FunctionAnalysis, MockConfig, TimingPhaseSummary } from "./protocol.js";
import type { InstrumentResult } from "./instrumentor.js";

// --- Main thread → Worker ---

export interface AnalyzeWorkerRequest {
  readonly id: number;
  readonly type: "analyze";
  readonly file: string;
  readonly functionName: string | null;
  readonly projectRoot: string | null;
}

export interface InstrumentWorkerRequest {
  readonly id: number;
  readonly type: "instrument";
  readonly source: string;
  readonly functionName: string;
  readonly fileName: string;
  readonly mocks: MockConfig[];
}

export type WorkerRequest = AnalyzeWorkerRequest | InstrumentWorkerRequest;

// --- Worker → Main thread ---

export interface AnalyzeWorkerResponse {
  readonly id: number;
  readonly type: "analyze";
  readonly functions: FunctionAnalysis[];
  readonly timingPhases?: TimingPhaseSummary[];
  readonly error?: string;
}

export interface InstrumentWorkerResponse {
  readonly id: number;
  readonly type: "instrument";
  readonly result: InstrumentResult | { error: string };
  readonly timingPhases?: TimingPhaseSummary[];
}

export type WorkerResponse = AnalyzeWorkerResponse | InstrumentWorkerResponse;
