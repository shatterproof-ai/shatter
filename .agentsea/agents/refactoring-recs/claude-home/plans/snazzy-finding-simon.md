# Chrome Extension: Migrate to TypeScript

## Context
The Chrome extension (`chrome-extension/`) is currently vanilla JS (4 files, ~208 lines) with no build step. Issue flt-c1w requests TypeScript migration with proper type safety and a build pipeline.

## Approach

### 1. Add build tooling
- Create `chrome-extension/package.json` with devDependencies: `typescript`, `@types/chrome`, `esbuild`
- Create `chrome-extension/tsconfig.json` targeting ES2022 (Chrome supports modern JS), strict mode
- Add esbuild build script that compiles each entry point to `dist/`

### 2. Convert JS → TS (4 files)
- `background/service-worker.js` → `src/background/service-worker.ts` — type the message handler, GraphQL response
- `popup/popup.js` → `src/popup/popup.ts` — type DOM element access, Chrome API calls
- `content/content.js` → `src/content/content.ts` — minimal, just type the return
- `options/options.js` → `src/options/options.ts` — type form elements, storage calls

### 3. Restructure for build output
- Move TS sources into `src/` subdirectory
- Keep static assets (HTML, CSS, icons) in place
- esbuild outputs compiled JS to `dist/` mirroring the original structure
- Copy static files (HTML, CSS, icons, manifest.json) to `dist/` during build
- Update manifest.json to work from `dist/` (paths stay relative, same structure)

### 4. Housekeeping
- Add `dist/` and `node_modules/` to `.gitignore`
- Update README with build instructions
- Delete original `.js` files after conversion

## Build Configuration

**esbuild** (chosen for speed, zero-config):
```
Entry points: src/background/service-worker.ts, src/popup/popup.ts, src/content/content.ts, src/options/options.ts
Output: dist/background/, dist/popup/, dist/content/, dist/options/
Format: esm for service worker, iife for others
Target: chrome120
```

**tsconfig.json**:
- `strict: true`, `target: "ES2022"`, `module: "ES2022"`
- `noEmit: true` (type-checking only, esbuild handles compilation)
- Types: `["chrome"]`

## Files to Create/Modify
- **Create**: `package.json`, `tsconfig.json`, `build.mjs`, `.gitignore`
- **Create**: `src/background/service-worker.ts`, `src/popup/popup.ts`, `src/content/content.ts`, `src/options/options.ts`
- **Modify**: `manifest.json` (no changes needed if dist mirrors structure)
- **Delete**: `background/service-worker.js`, `popup/popup.js`, `content/content.js`, `options/options.js`
- **Update**: `README.md` with build instructions

## Verification
1. `pnpm install` in chrome-extension/
2. `npx tsc --noEmit` — zero errors
3. `node build.mjs` — produces dist/ with all JS files
4. Inspect dist/ output is valid JS
5. No .js source files remain outside dist/
