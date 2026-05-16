/**
 * React hook recognizer fixture.
 *
 * useCounter: calls useState + useEffect → high confidence react-hook hint.
 * useFormattedName: calls useMemo → high confidence react-hook hint.
 * StatusCard: component that calls useState but no use* prefix → high confidence hint.
 * ContextPanel: component that calls useContext and a custom hook.
 * useFormatting: use* prefix but no hook calls → no hook hint.
 */

import { useState, useEffect, useMemo, useContext } from "react";

const ThemeContext = { _currentValue: "dark" };

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

function useAccentLabel(theme: string | undefined) {
  return theme === "dark" ? "contrast" : "plain";
}

export function ContextPanel(props: { title: string }) {
  const theme = useContext(ThemeContext);
  const label = useAccentLabel(theme);
  return <section data-label={label}>{props.title}</section>;
}

// No hint: useXxx name but no hook calls inside
export function useFormatting(text: string) {
  return text.toUpperCase();
}
