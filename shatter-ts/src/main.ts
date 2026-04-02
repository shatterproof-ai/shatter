#!/usr/bin/env node

/**
 * Shatter TypeScript frontend entry point.
 *
 * Reads newline-delimited JSON protocol messages from stdin and writes
 * responses to stdout. Debug output goes to stderr.
 */

import * as readline from "node:readline";
import { handleRequest, parseRequest } from "./handlers.js";
import logger from "./logger.js";
import { PROTOCOL_VERSION, type Response } from "./protocol.js";
import { serializeReplacer } from "./serialize.js";

function sendResponse(response: Response): void {
  const json = JSON.stringify(response, serializeReplacer);
  process.stdout.write(json + "\n");
  logger.trace({ raw: json }, "Sent");
}

function main(): void {
  logger.debug("Starting TypeScript frontend (protocol 0.1.0)");

  // Handle EPIPE on stdout gracefully.  When the core drops us after a
  // timeout it closes the pipe reader, and the next write emits an 'error'
  // event.  Without this handler Node crashes with an unhandled error.
  process.stdout.on("error", (err: NodeJS.ErrnoException) => {
    if (err.code === "EPIPE") {
      // Pipe closed by the core — exit silently.
      process.exit(0);
    }
    // Re-throw unexpected errors so they aren't silently swallowed.
    throw err;
  });

  const rl = readline.createInterface({
    input: process.stdin,
    terminal: false,
  });

  rl.on("line", (line: string) => {
    const trimmed = line.trim();
    if (trimmed === "") return;

    logger.trace({ raw: trimmed }, "Received");

    const result = parseRequest(trimmed);

    if ("error" in result) {
      sendResponse(result.error);
      return;
    }

    const requestId = result.request.id;
    void handleRequest(result.request).then(({ response, shutdown }) => {
      sendResponse(response);

      if (shutdown) {
        logger.debug("Shutting down");
        rl.close();
      }
    }).catch((err: unknown) => {
      const msg = err instanceof Error ? err.message : String(err);
      logger.error("Unhandled error processing request %s: %s", requestId, msg);
      sendResponse({
        protocol_version: PROTOCOL_VERSION,
        id: requestId,
        status: "error",
        code: "internal_error",
        message: `Unhandled error: ${msg}`,
      } as Response);
    });
  });

  rl.on("close", () => {
    logger.debug("Stdin closed, exiting");
    process.exit(0);
  });
}

main();
