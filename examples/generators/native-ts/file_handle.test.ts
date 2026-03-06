import { describe, it, afterEach } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import { ReadStream, WriteStream, Stats, teardown } from "./file_handle.js";

afterEach(() => {
  teardown();
});

describe("ReadStream generator", () => {
  it("returns an fs.ReadStream", () => {
    const result = ReadStream();
    assert.ok(result.value instanceof fs.ReadStream, "value should be an fs.ReadStream");
    assert.ok(result.id.startsWith("read-stream-"));
    assert.ok(result.recipe.tempPath);
    assert.ok(result.recipe.contentKind);
  });

  it("creates a readable temp file", () => {
    const result = ReadStream();
    assert.ok(fs.existsSync(result.recipe.tempPath));
  });

  it("replays from recipe", () => {
    const first = ReadStream();
    const recipe = first.recipe;
    const second = ReadStream(recipe);
    assert.equal(second.recipe.contentKind, recipe.contentKind);
    assert.ok(second.value instanceof fs.ReadStream);
  });
});

describe("WriteStream generator", () => {
  it("returns an fs.WriteStream", () => {
    const result = WriteStream();
    assert.ok(result.value instanceof fs.WriteStream, "value should be an fs.WriteStream");
    assert.ok(result.id.startsWith("write-stream-"));
  });

  it("creates a writable temp file", () => {
    const result = WriteStream();
    const stream = result.value as fs.WriteStream;
    stream.write("test data");
    stream.end();
  });
});

describe("Stats generator", () => {
  it("returns an fs.Stats object", () => {
    const result = Stats();
    assert.ok(result.value instanceof fs.Stats, "value should be an fs.Stats");
    assert.ok(result.id.startsWith("stats-"));
  });

  it("reflects correct file size for empty content", () => {
    const result = Stats({ tempPath: "", contentKind: "empty" });
    const stats = result.value as fs.Stats;
    assert.equal(stats.size, 0);
  });

  it("reflects correct file size for small_text content", () => {
    const result = Stats({ tempPath: "", contentKind: "small_text" });
    const stats = result.value as fs.Stats;
    assert.equal(stats.size, 45); // "The quick brown fox..." + newline
  });

  it("reflects correct file size for binary content", () => {
    const result = Stats({ tempPath: "", contentKind: "binary" });
    const stats = result.value as fs.Stats;
    assert.equal(stats.size, 1024);
  });

  it("reflects correct file size for large content", () => {
    const result = Stats({ tempPath: "", contentKind: "large" });
    const stats = result.value as fs.Stats;
    assert.ok(stats.size > 60000, `Expected large file >60KB, got ${stats.size}`);
  });
});

describe("content kind cycling", () => {
  it("cycles through all content kinds", () => {
    teardown(); // Reset index
    const kinds = [
      ReadStream().recipe.contentKind,
      WriteStream().recipe.contentKind,
      Stats().recipe.contentKind,
      ReadStream().recipe.contentKind,
    ];
    assert.deepEqual(kinds, ["empty", "small_text", "binary", "large"]);
  });
});

describe("teardown", () => {
  it("removes all temp files", () => {
    const r1 = ReadStream();
    const r2 = WriteStream();
    const r3 = Stats();
    const paths = [r1.recipe.tempPath, r2.recipe.tempPath, r3.recipe.tempPath];

    for (const p of paths) {
      assert.ok(fs.existsSync(p), `File should exist before teardown: ${p}`);
    }

    teardown();

    for (const p of paths) {
      assert.ok(!fs.existsSync(p), `File should be removed after teardown: ${p}`);
    }
  });

  it("handles already-removed files gracefully", () => {
    const result = ReadStream();
    fs.unlinkSync(result.recipe.tempPath);
    assert.doesNotThrow(() => teardown());
  });
});
