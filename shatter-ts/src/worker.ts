/**
 * Instrumentation worker thread entry point.
 *
 * Handles CPU-bound analyze and instrument operations off the main thread.
 * Imports analyzer and instrumentor eagerly — this thread's entire purpose
 * is to run these heavy modules.
 */

import { parentPort } from "node:worker_threads";
import { analyzeFile } from "./analyzer.js";
import { instrumentFunction } from "./instrumentor.js";
import { TimingCollector } from "./timing.js";
import type { WorkerRequest, WorkerResponse } from "./worker-protocol.js";

if (!parentPort) {
  throw new Error("worker.ts must be run as a worker_thread");
}

const port = parentPort;

port.on("message", (msg: WorkerRequest) => {
  switch (msg.type) {
    case "analyze": {
      try {
        const timing = new TimingCollector();
        const functions = timing.sync("analyze.total", () =>
          analyzeFile(msg.file, msg.functionName, msg.projectRoot, timing),
        );
        const summary = timing.toSummary();
        const response: WorkerResponse = {
          id: msg.id,
          type: "analyze",
          functions,
          timingPhases: summary?.phases,
        };
        port.postMessage(response);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        const response: WorkerResponse = {
          id: msg.id,
          type: "analyze",
          functions: [],
          error: message,
        };
        port.postMessage(response);
      }
      break;
    }

    case "instrument": {
      try {
        const timing = new TimingCollector();
        const result = timing.sync("instrument.total", () =>
          instrumentFunction(msg.source, msg.functionName, msg.fileName, msg.mocks, timing),
        );
        const summary = timing.toSummary();
        const response: WorkerResponse = {
          id: msg.id,
          type: "instrument",
          result,
          timingPhases: summary?.phases,
        };
        port.postMessage(response);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        const response: WorkerResponse = {
          id: msg.id,
          type: "instrument",
          result: { error: message },
        };
        port.postMessage(response);
      }
      break;
    }
  }
});
