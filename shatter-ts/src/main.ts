#!/usr/bin/env node

/**
 * Shatter TypeScript frontend entry point.
 *
 * Reads newline-delimited JSON protocol messages from stdin and writes
 * responses to stdout. Debug output goes to stderr.
 */

import * as readline from "node:readline";
import { handleRequest, parseRequest } from "./handlers.js";
import type { Response } from "./protocol.js";

function log(message: string): void {
  process.stderr.write(`[shatter-ts] ${message}\n`);
}

function sendResponse(response: Response): void {
  const json = JSON.stringify(response);
  process.stdout.write(json + "\n");
  log(`Sent: ${json}`);
}

function main(): void {
  log("Starting TypeScript frontend (protocol 0.1.0)");

  const rl = readline.createInterface({
    input: process.stdin,
    terminal: false,
  });

  rl.on("line", (line: string) => {
    const trimmed = line.trim();
    if (trimmed === "") return;

    log(`Received: ${trimmed}`);

    const result = parseRequest(trimmed);

    if ("error" in result) {
      sendResponse(result.error);
      return;
    }

    const { response, shutdown } = handleRequest(result.request);
    sendResponse(response);

    if (shutdown) {
      log("Shutting down");
      rl.close();
    }
  });

  rl.on("close", () => {
    log("Stdin closed, exiting");
    process.exit(0);
  });
}

main();
