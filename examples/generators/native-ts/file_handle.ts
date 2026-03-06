// Native TypeScript generator for file handle opaque types.
//
// Produces live fs.ReadStream, fs.WriteStream, and fs.Stats objects backed
// by temp files with varied content. Cleanup via teardown().
//
// Usage: place at .shatter/generators/file_handle.ts and reference in config.yaml:
//
//   defaults:
//     generators:
//       ReadStream: .shatter/generators/file_handle.ts
//       WriteStream: .shatter/generators/file_handle.ts
//       Stats: .shatter/generators/file_handle.ts

import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as crypto from "node:crypto";

// --- Content generation ---

const CONTENT_KINDS = ["empty", "small_text", "binary", "large"] as const;
type ContentKind = (typeof CONTENT_KINDS)[number];

interface FileHandleRecipe {
  tempPath: string;
  contentKind: ContentKind;
}

interface GeneratorResult {
  value: unknown;
  id: string;
  recipe: FileHandleRecipe;
}

let contentIndex = 0;

function nextContentKind(): ContentKind {
  const kind = CONTENT_KINDS[contentIndex % CONTENT_KINDS.length];
  contentIndex++;
  return kind;
}

function generateContent(kind: ContentKind): Buffer {
  switch (kind) {
    case "empty":
      return Buffer.alloc(0);
    case "small_text":
      return Buffer.from("The quick brown fox jumps over the lazy dog.\n");
    case "binary":
      return crypto.randomBytes(1024);
    case "large":
      return Buffer.from("abcdefghijklmnopqrstuvwxyz0123456789\n".repeat(1820));
  }
}

// --- Temp file tracking for cleanup ---

const trackedPaths = new Set<string>();
const trackedStreams = new Set<fs.ReadStream | fs.WriteStream>();

function createTempFile(kind: ContentKind): string {
  const tempDir = os.tmpdir();
  const name = `shatter-gen-${crypto.randomBytes(8).toString("hex")}`;
  const tempPath = path.join(tempDir, name);
  fs.writeFileSync(tempPath, generateContent(kind));
  trackedPaths.add(tempPath);
  return tempPath;
}

function ensureTempFile(recipe: FileHandleRecipe): string {
  if (fs.existsSync(recipe.tempPath)) {
    trackedPaths.add(recipe.tempPath);
    return recipe.tempPath;
  }
  // Recipe path gone — recreate with same content kind
  const tempPath = createTempFile(recipe.contentKind);
  return tempPath;
}

// --- Generator functions ---

/** Generate an fs.ReadStream backed by a temp file with varied content. */
export function ReadStream(recipe?: FileHandleRecipe): GeneratorResult {
  const contentKind = recipe?.contentKind ?? nextContentKind();
  const tempPath = recipe ? ensureTempFile(recipe) : createTempFile(contentKind);
  // Open fd eagerly so the stream doesn't try to open the file asynchronously
  // after teardown deletes it.
  const fd = fs.openSync(tempPath, "r");
  const stream = fs.createReadStream("", { fd, autoClose: true });
  trackedStreams.add(stream);

  return {
    value: stream,
    id: `read-stream-${path.basename(tempPath)}`,
    recipe: { tempPath, contentKind },
  };
}

/** Generate an fs.WriteStream backed by a temp file. */
export function WriteStream(recipe?: FileHandleRecipe): GeneratorResult {
  const contentKind = recipe?.contentKind ?? nextContentKind();
  const tempPath = recipe ? ensureTempFile(recipe) : createTempFile(contentKind);
  const stream = fs.createWriteStream(tempPath);
  trackedStreams.add(stream);

  return {
    value: stream,
    id: `write-stream-${path.basename(tempPath)}`,
    recipe: { tempPath, contentKind },
  };
}

/** Generate an fs.Stats object from a temp file with varied content. */
export function Stats(recipe?: FileHandleRecipe): GeneratorResult {
  const contentKind = recipe?.contentKind ?? nextContentKind();
  const tempPath = recipe ? ensureTempFile(recipe) : createTempFile(contentKind);
  const stats = fs.statSync(tempPath);

  return {
    value: stats,
    id: `stats-${path.basename(tempPath)}`,
    recipe: { tempPath, contentKind },
  };
}

/** Clean up all tracked streams and temp files. */
export function teardown(): void {
  for (const stream of trackedStreams) {
    try {
      stream.destroy();
    } catch {
      // Stream may already be closed
    }
  }
  trackedStreams.clear();

  for (const tempPath of trackedPaths) {
    try {
      fs.unlinkSync(tempPath);
    } catch {
      // File may already be removed
    }
  }
  trackedPaths.clear();

  contentIndex = 0;
}
