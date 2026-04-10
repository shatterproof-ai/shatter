/**
 * React hook recognizer fixture.
 *
 * useCounter: calls useState + useEffect → high confidence react-hook hint.
 * useFormattedName: calls useMemo → high confidence react-hook hint.
 * StatusCard: component that calls useState but no use* prefix → no hook hint.
 * useFormatting: use* prefix but no hook calls → no hook hint.
 */

import { useState, useEffect, useMemo } from "react";

// High confidence: calls builtin hooks, follows useXxx naming
export function useCounter(initial: number) {
  const [count, setCount] = useState(initial);
  useEffect(() => {
    /* track count */
  }, [count]);
  return count;
}

// High confidence: calls useMemo from react
export function useFormattedName(first: string, last: string) {
  return useMemo(() => `${first} ${last}`, [first, last]);
}

// No hint: component (uses hooks but name is not useXxx)
export function StatusCard(props: { status: string }) {
  const [val] = useState(0);
  if (props.status === "active") return "active";
  return "inactive";
}

// No hint: useXxx name but no hook calls inside
export function useFormatting(text: string) {
  return text.toUpperCase();
}
