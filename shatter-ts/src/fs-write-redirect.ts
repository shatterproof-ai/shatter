/**
 * `require('fs')` path-redirection shim for host-write isolation (str-02i70).
 *
 * Companion to the Go/Rust frontends' `SHATTER_HOST_WRITE_DIR` isolation
 * (str-gg9v): when the operator opts into unsandboxed target execution
 * (`--allow-host-writes` / `SHATTER_ALLOW_HOST_WRITES=1`), the `shatter` CLI
 * exports `SHATTER_HOST_WRITE_DIR` — an absolute path to a throwaway
 * directory — into the frontend's environment. Go and Rust redirect their
 * target-execution *subprocess*'s working directory to that throwaway
 * directory (`cmd.Dir` / `Command::current_dir`), so relative-path writes
 * land there instead of the invoking repository.
 *
 * TypeScript has no equivalent subprocess boundary: target code runs
 * in-process via `vm.runInContext`, and `main.ts` dispatches requests
 * concurrently (fires `handleRequest` without awaiting), so a global
 * `process.chdir()` would race across concurrent invocations and is unsafe
 * (confirmed by a prior attempt that broke relative source-path resolution
 * by chdir'ing the whole frontend process). Instead, this module intercepts
 * `require('fs')` / `require('node:fs')` / `require('fs/promises')` /
 * `require('node:fs/promises')` inside the sandbox and rewrites the path
 * argument of *mutating* fs calls (write/create/rename/delete/permission
 * changes) to resolve under the throwaway directory instead of the process
 * cwd — scoped per require() call (reads `SHATTER_HOST_WRITE_DIR` fresh
 * each time, never cached at module scope), so concurrent dispatches never
 * interfere with each other. Read-only calls are left untouched: a target's
 * genuine relative reads of repo fixtures keep working.
 *
 * Registered as a `ResolverAdapter` in `executor.ts`'s
 * `getDefaultResolverAdapters`, the same mechanism the React shim uses to
 * intercept `require('react')`.
 */

import * as path from "node:path";

/** Env var carrying the throwaway host-write isolation directory. Mirrors
 *  `shatter-cli/src/host_writes.rs`'s `ISOLATION_DIR_ENV` and Go's
 *  `hostWriteDirEnv` (`shatter-go/protocol/prepared_launcher.go`). */
export const HOST_WRITE_DIR_ENV = "SHATTER_HOST_WRITE_DIR";

/** Module specifiers intercepted by the require wrapper. */
export const FS_MODULE_IDS = new Set([
  "fs",
  "node:fs",
  "fs/promises",
  "node:fs/promises",
]);

/**
 * Return the configured host-write isolation directory, or `undefined` when
 * unset/blank. Read fresh on every call (never cached) so each require()
 * call picks up the current per-invocation value — no module-level state
 * that could leak across concurrent dispatches.
 */
export function getHostWriteDir(): string | undefined {
  const value = process.env[HOST_WRITE_DIR_ENV];
  return value && value.trim().length > 0 ? value : undefined;
}

/**
 * Resolve a candidate fs path argument under `hostWriteDir` when it is a
 * relative string path. Absolute paths, non-string values (Buffer, URL, file
 * descriptor numbers), and everything else pass through unchanged — this
 * shim only redirects the common "relative string path" case that produces
 * stray files in the invoking repo; it does not attempt to rewrite exotic
 * path representations.
 *
 * `path.resolve` alone does not stop a `..`-laden relative path (routine in
 * literal-mined/generated string inputs, per `host_writes.rs`'s threat
 * model) from walking back out of `hostWriteDir` to the real filesystem —
 * that would defeat the shim entirely. After resolving, verify the result is
 * still contained in `hostWriteDir`; if it escaped, redirect to the
 * resolved path's basename joined under `hostWriteDir` instead, so the write
 * always lands inside the throwaway directory.
 */
function redirectPathValue(value: unknown, hostWriteDir: string): unknown {
  if (typeof value !== "string" || path.isAbsolute(value)) return value;
  const resolvedHostWriteDir = path.resolve(hostWriteDir);
  const resolved = path.resolve(resolvedHostWriteDir, value);
  const relative = path.relative(resolvedHostWriteDir, resolved);
  const escapesHostWriteDir =
    relative === ".." || relative.startsWith(`..${path.sep}`);
  if (!escapesHostWriteDir) return resolved;
  return path.join(resolvedHostWriteDir, path.basename(resolved));
}

function redirectArgs(
  args: unknown[],
  pathArgIndices: readonly number[],
  hostWriteDir: string,
): unknown[] {
  const next = args.slice();
  for (const index of pathArgIndices) {
    if (index < next.length) {
      next[index] = redirectPathValue(next[index], hostWriteDir);
    }
  }
  return next;
}

/** Wrap an fs function so the path argument(s) at `pathArgIndices` resolve
 *  under `hostWriteDir` when relative. Transparent to sync/callback/promise
 *  call styles — only the path argument(s) are touched. */
function wrapPathArgs(
  original: unknown,
  pathArgIndices: readonly number[],
  hostWriteDir: string,
): unknown {
  if (typeof original !== "function") return original;
  const fn = original as (...fnArgs: unknown[]) => unknown;
  return function (this: unknown, ...args: unknown[]): unknown {
    return fn.apply(this, redirectArgs(args, pathArgIndices, hostWriteDir));
  };
}

/** `open`/`openSync`/`promises.open` only mutate when opened with a
 *  write-capable flag. Redirect the path argument unless the flags argument
 *  is unambiguously read-only (omitted, `"r"`, or the numeric `O_RDONLY`
 *  constant) — conservative in favor of redirecting, matching the "opens,
 *  creates, renames, or deletes" scope `host_writes.rs` documents. */
function wrapOpenFn(
  original: unknown,
  hostWriteDir: string,
  oRdonly: number | undefined,
): unknown {
  if (typeof original !== "function") return original;
  const fn = original as (...fnArgs: unknown[]) => unknown;
  return function (this: unknown, ...args: unknown[]): unknown {
    const flags = args[1];
    const isReadOnly =
      flags === undefined ||
      flags === "r" ||
      (typeof flags === "number" && flags === oRdonly);
    const nextArgs = isReadOnly ? args : redirectArgs(args, [0], hostWriteDir);
    return fn.apply(this, nextArgs);
  };
}

/**
 * Mutating fs entry points and the argument indices holding a path that
 * should redirect when relative. Two-arg specs use both indices when both
 * sides of the call are mutating (e.g. `rename`); single-arg specs redirect
 * only the destination/created path, leaving a genuine source read (e.g.
 * `copyFile`'s source, `link`/`symlink`'s existing target) untouched.
 *
 * `open`/`openSync` are handled separately by `wrapOpenFn` since redirection
 * depends on the flags argument, not just the path.
 */
const WRITE_FN_SPECS: ReadonlyArray<{
  readonly name: string;
  readonly pathArgIndices: readonly number[];
}> = [
  { name: "writeFile", pathArgIndices: [0] },
  { name: "appendFile", pathArgIndices: [0] },
  { name: "truncate", pathArgIndices: [0] },
  { name: "unlink", pathArgIndices: [0] },
  { name: "rmdir", pathArgIndices: [0] },
  { name: "mkdir", pathArgIndices: [0] },
  { name: "mkdtemp", pathArgIndices: [0] },
  { name: "rm", pathArgIndices: [0] },
  { name: "cp", pathArgIndices: [1] },
  { name: "chmod", pathArgIndices: [0] },
  { name: "lchmod", pathArgIndices: [0] },
  { name: "chown", pathArgIndices: [0] },
  { name: "lchown", pathArgIndices: [0] },
  { name: "utimes", pathArgIndices: [0] },
  { name: "lutimes", pathArgIndices: [0] },
  { name: "rename", pathArgIndices: [0, 1] },
  { name: "copyFile", pathArgIndices: [1] },
  { name: "link", pathArgIndices: [1] },
  { name: "symlink", pathArgIndices: [1] },
  { name: "createWriteStream", pathArgIndices: [0] },
];

/** Apply write-path redirection in place to an fs-shaped module object
 *  (either the top-level `fs` module or its `fs/promises` counterpart —
 *  both export the same mutating-function names). */
function applyWriteRedirect(
  target: Record<string, unknown>,
  hostWriteDir: string,
  oRdonly: number | undefined,
): void {
  for (const spec of WRITE_FN_SPECS) {
    if (spec.name in target) {
      target[spec.name] = wrapPathArgs(
        target[spec.name],
        spec.pathArgIndices,
        hostWriteDir,
      );
    }
    const syncName = `${spec.name}Sync`;
    if (syncName in target) {
      target[syncName] = wrapPathArgs(
        target[syncName],
        spec.pathArgIndices,
        hostWriteDir,
      );
    }
  }
  if ("open" in target) {
    target["open"] = wrapOpenFn(target["open"], hostWriteDir, oRdonly);
  }
  if ("openSync" in target) {
    target["openSync"] = wrapOpenFn(target["openSync"], hostWriteDir, oRdonly);
  }
}

/**
 * Build a shim for `moduleId` (one of `FS_MODULE_IDS`) that redirects
 * mutating calls' relative path arguments under `hostWriteDir`. Everything
 * else — reads, constants, stream classes, etc. — is passed through from the
 * real module unchanged.
 */
export function buildFsWriteRedirectShim(
  moduleId: string,
  originalRequire: NodeRequire,
  hostWriteDir: string,
): Record<string, unknown> {
  // Always resolve `constants` from the real `fs` module: `fs/promises` does
  // not reliably re-export it across supported Node versions, and `open`'s
  // read-only check needs `O_RDONLY` regardless of which variant was
  // required.
  const realFs = originalRequire("node:fs") as Record<string, unknown>;
  const fsConstants = realFs["constants"] as Record<string, unknown> | undefined;
  const oRdonly =
    typeof fsConstants?.["O_RDONLY"] === "number"
      ? (fsConstants["O_RDONLY"] as number)
      : undefined;

  const real = originalRequire(moduleId) as Record<string, unknown>;
  const shim: Record<string, unknown> = { ...real };
  applyWriteRedirect(shim, hostWriteDir, oRdonly);

  // `fs.promises` is the same promise-based API surface as `fs/promises`;
  // shim it too so `fs.promises.writeFile(...)` redirects identically to
  // `require('fs/promises').writeFile(...)`.
  const promises = shim["promises"];
  if (promises && typeof promises === "object") {
    const promisesShim: Record<string, unknown> = {
      ...(promises as Record<string, unknown>),
    };
    applyWriteRedirect(promisesShim, hostWriteDir, oRdonly);
    shim["promises"] = promisesShim;
  }

  return shim;
}
