/**
 * Main-thread manager for the instrumentation worker thread.
 *
 * Provides async request-response wrappers around the worker's postMessage
 * protocol. Correlates requests and responses via auto-incrementing IDs.
 */

import * as path from "node:path";
import { Worker } from "node:worker_threads";
import type { FunctionAnalysis, TimingPhaseSummary } from "./protocol.js";
import type { InstrumentResult } from "./instrumentor.js";
import type {
  WorkerRequest,
  WorkerResponse,
  AnalyzeWorkerResponse,
  InstrumentWorkerResponse,
} from "./worker-protocol.js";

interface PendingRequest {
  readonly resolve: (result: WorkerResponse) => void;
  readonly reject: (err: Error) => void;
}

export interface AnalyzeResult {
  readonly functions: FunctionAnalysis[];
  readonly timingPhases?: TimingPhaseSummary[];
}

export interface InstrumentWorkerResult {
  readonly result: InstrumentResult | { error: string };
  readonly timingPhases?: TimingPhaseSummary[];
}

/**
 * Manages a single worker thread for CPU-bound analyze and instrument work.
 * The worker imports analyzer.ts and instrumentor.ts eagerly on startup.
 */
export class InstrumentationWorker {
  private worker: Worker;
  private nextId = 0;
  private readonly pending = new Map<number, PendingRequest>();

  constructor(workerPath?: string) {
    // Default: resolve to dist/worker.js relative to this file's compiled location.
    // When running from src/ under ts-jest, __dirname is src/ — but the worker
    // must always point to compiled JS. The workerPath override handles test scenarios.
    const resolved = workerPath ?? path.join(__dirname, "worker.js");
    this.worker = new Worker(resolved);

    this.worker.on("message", (msg: WorkerResponse) => {
      const p = this.pending.get(msg.id);
      if (p) {
        this.pending.delete(msg.id);
        p.resolve(msg);
      }
    });

    this.worker.on("error", (err: Error) => {
      for (const [, p] of this.pending) {
        p.reject(err);
      }
      this.pending.clear();
    });
  }

  /** Send an analyze request to the worker and await the result. */
  async analyze(
    file: string,
    functionName: string | null,
    projectRoot: string | null,
  ): Promise<AnalyzeResult> {
    const id = this.nextId++;
    const request: WorkerRequest = {
      id,
      type: "analyze",
      file,
      functionName,
      projectRoot,
    };

    const response = await this.send(request) as AnalyzeWorkerResponse;
    if (response.error) {
      throw new Error(response.error);
    }
    return {
      functions: response.functions,
      timingPhases: response.timingPhases,
    };
  }

  /** Send an instrument request to the worker and await the result. */
  async instrument(
    source: string,
    functionName: string,
    fileName: string,
    mocks: ReadonlyArray<import("./protocol.js").MockConfig>,
  ): Promise<InstrumentWorkerResult> {
    const id = this.nextId++;
    const request: WorkerRequest = {
      id,
      type: "instrument",
      source,
      functionName,
      fileName,
      mocks: [...mocks],
    };

    const response = await this.send(request) as InstrumentWorkerResponse;
    return {
      result: response.result,
      timingPhases: response.timingPhases,
    };
  }

  /** Terminate the worker thread. */
  async terminate(): Promise<void> {
    for (const [, p] of this.pending) {
      p.reject(new Error("Worker terminated"));
    }
    this.pending.clear();
    await this.worker.terminate();
  }

  private send(request: WorkerRequest): Promise<WorkerResponse> {
    return new Promise<WorkerResponse>((resolve, reject) => {
      this.pending.set(request.id, { resolve, reject });
      this.worker.postMessage(request);
    });
  }
}
