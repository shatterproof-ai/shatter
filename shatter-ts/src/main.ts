#!/usr/bin/env node

/**
 * Shatter TypeScript frontend entry point.
 *
 * Reads newline-delimited JSON protocol messages from stdin and writes
 * responses to stdout. Debug output goes to stderr.
 */

import * as readline from "node:readline";
import { handleRequest, parseRequest } from "./handlers.js";
import { PROTOCOL_VERSION, type Response } from "./protocol.js";

type FrontendLogLevel = "error" | "warn" | "info" | "debug" | "trace";

const LOG_LEVEL_RANK: Record<FrontendLogLevel, number> = {
  error: 0,
  warn: 1,
  info: 2,
  debug: 3,
  trace: 4,
};

function getLogLevel(): FrontendLogLevel {
  const env = process.env["SHATTER_LOG_LEVEL"]?.toLowerCase();
  if (env !== undefined && env in LOG_LEVEL_RANK) {
    return env as FrontendLogLevel;
  }
  return "info";
}

const currentLogLevel = getLogLevel();

function shouldLog(level: FrontendLogLevel): boolean {
  return LOG_LEVEL_RANK[currentLogLevel] >= LOG_LEVEL_RANK[level];
}

function log(message: string, level: FrontendLogLevel = "trace"): void {
  if (shouldLog(level)) {
    process.stderr.write(`[shatter-ts] ${message}\n`);
  }
}

function sendResponse(response: Response): void {
  const json = JSON.stringify(response);
  process.stdout.write(json + "\n");
  log(`Sent: ${json}`, "trace");
}

function main(): void {
  log("Starting TypeScript frontend (protocol 0.1.0)", "debug");

  const rl = readline.createInterface({
    input: process.stdin,
    terminal: false,
  });

  rl.on("line", (line: string) => {
    const trimmed = line.trim();
    if (trimmed === "") return;

    log(`Received: ${trimmed}`, "trace");

    const result = parseRequest(trimmed);

    if ("error" in result) {
      sendResponse(result.error);
      return;
    }

    const requestId = result.request.id;
    void handleRequest(result.request).then(({ response, shutdown }) => {
      sendResponse(response);

      if (shutdown) {
        log("Shutting down", "debug");
        rl.close();
      }
    }).catch((err: unknown) => {
      const msg = err instanceof Error ? err.message : String(err);
      log(`Unhandled error processing request ${requestId}: ${msg}`, "error");
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
    log("Stdin closed, exiting", "debug");
    process.exit(0);
  });
}

main();
