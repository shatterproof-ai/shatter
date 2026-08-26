/**
 * Adapter fixture: react-hook module reached through a real (non-type-only)
 * import cycle, reproducing str-aatcq ("Maximum call stack size exceeded").
 *
 * component.ts <-> store.ts form a cycle: this file imports a value
 * (STORE_LABEL) from store.ts at module top level, and store.ts imports this
 * file's useBookmarkButton back for its default-label fallback. This mirrors
 * the real-world pattern (a component + its co-located zustand/jotai store
 * module, or a barrel `index.ts` re-exporting a component that itself imports
 * the barrel) that triggered the crash in kapow's BookmarkButton. Kept as
 * plain `.ts` (no JSX syntax needed to reproduce) so the fixture doesn't
 * depend on jsx compiler options.
 *
 * useBookmarkButton: calls useCallback (react-hook adapter trigger) and reads
 * a value re-exported through the cycle.
 */
import { useCallback } from "react";
import { STORE_LABEL } from "./store.js";

export function useBookmarkButton(bookmarked: boolean) {
  const toggle = useCallback(() => !bookmarked, [bookmarked]);
  return { label: bookmarked ? STORE_LABEL : "Bookmark", toggle };
}
