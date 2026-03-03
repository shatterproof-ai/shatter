import {
  loadWasmPlugin,
  runWasmGenerator,
  clearWasmCache,
  wasmCacheSize,
} from "./wasm-generator.js";

describe("wasm-generator", () => {
  afterEach(async () => {
    await clearWasmCache();
  });

  describe("loadWasmPlugin", () => {
    it("throws for missing WASM file", async () => {
      await expect(loadWasmPlugin("/nonexistent/plugin.wasm")).rejects.toThrow(
        /ENOENT|no such file/,
      );
    });

    it("caches plugins by path", async () => {
      expect(wasmCacheSize()).toBe(0);

      // Loading a missing file does not pollute the cache
      try {
        await loadWasmPlugin("/nonexistent/plugin.wasm");
      } catch {
        // expected
      }
      expect(wasmCacheSize()).toBe(0);
    });
  });

  describe("clearWasmCache", () => {
    it("resets cache size to zero", async () => {
      // Just verify the function runs without error on an empty cache
      await clearWasmCache();
      expect(wasmCacheSize()).toBe(0);
    });
  });

  describe("runWasmGenerator", () => {
    it("throws when plugin call returns null", async () => {
      const mockPlugin = {
        call: jest.fn().mockResolvedValue(null),
        functionExists: jest.fn().mockResolvedValue(true),
        close: jest.fn().mockResolvedValue(undefined),
        getExports: jest.fn(),
        getImports: jest.fn(),
        getInstance: jest.fn(),
        isActive: jest.fn().mockReturnValue(false),
        reset: jest.fn(),
      };

      await expect(
        runWasmGenerator(mockPlugin, "generate_user"),
      ).rejects.toThrow(/returned null output/);
    });

    it("throws when plugin returns invalid JSON shape", async () => {
      const mockOutput = {
        text: () => JSON.stringify({ wrong: "shape" }),
        buffer: new ArrayBuffer(0),
      };
      const mockPlugin = {
        call: jest.fn().mockResolvedValue(mockOutput),
        functionExists: jest.fn().mockResolvedValue(true),
        close: jest.fn().mockResolvedValue(undefined),
        getExports: jest.fn(),
        getImports: jest.fn(),
        getInstance: jest.fn(),
        isActive: jest.fn().mockReturnValue(false),
        reset: jest.fn(),
      };

      await expect(
        runWasmGenerator(mockPlugin, "generate_user"),
      ).rejects.toThrow(/invalid JSON/);
    });

    it("parses valid generator output with value and id", async () => {
      const expectedResult = { value: { name: "Alice" }, id: "user-gen-1" };
      const mockOutput = {
        text: () => JSON.stringify(expectedResult),
        buffer: new ArrayBuffer(0),
      };
      const mockPlugin = {
        call: jest.fn().mockResolvedValue(mockOutput),
        functionExists: jest.fn().mockResolvedValue(true),
        close: jest.fn().mockResolvedValue(undefined),
        getExports: jest.fn(),
        getImports: jest.fn(),
        getInstance: jest.fn(),
        isActive: jest.fn().mockReturnValue(false),
        reset: jest.fn(),
      };

      const result = await runWasmGenerator(mockPlugin, "generate_user");
      expect(result.value).toEqual({ name: "Alice" });
      expect(result.id).toBe("user-gen-1");
      expect(result.recipe).toBeUndefined();
    });

    it("includes recipe when present in output", async () => {
      const expectedResult = {
        value: 42,
        id: "int-gen",
        recipe: { seed: 7 },
      };
      const mockOutput = {
        text: () => JSON.stringify(expectedResult),
        buffer: new ArrayBuffer(0),
      };
      const mockPlugin = {
        call: jest.fn().mockResolvedValue(mockOutput),
        functionExists: jest.fn().mockResolvedValue(true),
        close: jest.fn().mockResolvedValue(undefined),
        getExports: jest.fn(),
        getImports: jest.fn(),
        getInstance: jest.fn(),
        isActive: jest.fn().mockReturnValue(false),
        reset: jest.fn(),
      };

      const result = await runWasmGenerator(mockPlugin, "generate_int");
      expect(result.value).toBe(42);
      expect(result.id).toBe("int-gen");
      expect(result.recipe).toEqual({ seed: 7 });
    });

    it("passes recipe as JSON input when provided", async () => {
      const mockOutput = {
        text: () => JSON.stringify({ value: "ok", id: "test" }),
        buffer: new ArrayBuffer(0),
      };
      const mockPlugin = {
        call: jest.fn().mockResolvedValue(mockOutput),
        functionExists: jest.fn().mockResolvedValue(true),
        close: jest.fn().mockResolvedValue(undefined),
        getExports: jest.fn(),
        getImports: jest.fn(),
        getInstance: jest.fn(),
        isActive: jest.fn().mockReturnValue(false),
        reset: jest.fn(),
      };

      const recipe = { seed: 42 };
      await runWasmGenerator(mockPlugin, "gen", recipe);

      expect(mockPlugin.call).toHaveBeenCalledWith("gen", JSON.stringify(recipe));
    });

    it("passes empty string input when no recipe provided", async () => {
      const mockOutput = {
        text: () => JSON.stringify({ value: "ok", id: "test" }),
        buffer: new ArrayBuffer(0),
      };
      const mockPlugin = {
        call: jest.fn().mockResolvedValue(mockOutput),
        functionExists: jest.fn().mockResolvedValue(true),
        close: jest.fn().mockResolvedValue(undefined),
        getExports: jest.fn(),
        getImports: jest.fn(),
        getInstance: jest.fn(),
        isActive: jest.fn().mockReturnValue(false),
        reset: jest.fn(),
      };

      await runWasmGenerator(mockPlugin, "gen");

      expect(mockPlugin.call).toHaveBeenCalledWith("gen", "");
    });
  });
});
