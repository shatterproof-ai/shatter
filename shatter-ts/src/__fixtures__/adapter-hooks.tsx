/**
 * Adapter fixture: React hooks requiring adapter intervention.
 *
 * These functions call React hooks that need a React runtime shim to execute
 * without crashing. The adapter layer must provide the shim before execution.
 *
 * useToggle: calls useState → needs react-hook adapter.
 *   - initial === true  → starts toggled on
 *   - initial === false → starts toggled off
 *
 * useGreeting: calls useMemo → needs react-hook adapter.
 *   - name is truthy  → personalized greeting
 *   - name is falsy   → default greeting
 *
 * useDebounced: calls useState + useEffect → needs react-hook adapter.
 *   - delay > 0 → debounced update path
 *   - delay <= 0 → immediate update path
 *
 * useCounter: calls useState → needs react-hook adapter.
 *   - Increments/decrements count via setter
 *   - Good for verifying multi-step state transitions
 *
 * useDocTitle: calls useState + useEffect → needs react-hook adapter.
 *   - title is truthy → effect sets title
 *   - title is falsy → effect sets default
 *
 * useAsyncEffect: calls useEffect with async callback → must throw.
 *
 * plainHelper: no hooks → should NOT trigger react-hook adapter.
 */

import { useState, useEffect, useMemo } from "react";

export function useToggle(initial: boolean) {
  const [on, setOn] = useState(initial);
  if (on) {
    return { state: "on", toggle: () => setOn(false) };
  }
  return { state: "off", toggle: () => setOn(true) };
}

export function useGreeting(name: string) {
  const greeting = useMemo(() => {
    if (name) {
      return `Hello, ${name}!`;
    }
    return "Hello, stranger!";
  }, [name]);
  return greeting;
}

export function useDebounced(value: number, delay: number) {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    if (delay > 0) {
      const timer = setTimeout(() => setDebounced(value), delay);
      return () => clearTimeout(timer);
    }
    setDebounced(value);
    return undefined;
  }, [value, delay]);
  return debounced;
}

export function useCounter(initial: number) {
  const [count, setCount] = useState(initial);
  return {
    count,
    increment: () => setCount(count + 1),
    decrement: () => setCount(count - 1),
  };
}

/**
 * useDocTitle: calls useEffect to conditionally set a document title.
 *   - title is truthy → effect sets title
 *   - title is falsy → effect sets default title
 * Exercises effect callback branches.
 */
export function useDocTitle(title: string) {
  const [current, setCurrent] = useState(title || "Untitled");
  useEffect(() => {
    if (title) {
      setCurrent(title);
    } else {
      setCurrent("Untitled");
    }
  }, [title]);
  return current;
}

/**
 * useAsyncEffect: calls useEffect with an async callback.
 * This is intentionally unsupported and should throw UnsupportedEffectError.
 */
export function useAsyncEffect() {
  const [value] = useState(0);
  useEffect(() => {
    return Promise.resolve() as unknown as void;
  }, []);
  return value;
}

export function plainHelper(x: number): number {
  return x * 2;
}
