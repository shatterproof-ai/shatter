/**
 * React context fixture for str-lu2.9.
 *
 * Exercises the createContext → Provider → useContext flow that previously
 * crashed with `TypeError: (0 , react_1.createContext) is not a function`.
 *
 * `themedLabel` branches on the context value so the concolic engine sees
 * a real decision point rather than a constant pass-through.
 */

import { createContext, useContext } from "react";

export const ThemeContext = createContext<"light" | "dark">("light");

export function themedLabel(): string {
  const theme = useContext(ThemeContext);
  if (theme === "dark") {
    return "contrast";
  }
  return "plain";
}

export function AuthProvider(props: { value: "light" | "dark"; children?: unknown }) {
  return <ThemeContext.Provider value={props.value}>{props.children}</ThemeContext.Provider>;
}

export function useAuth(): string {
  return themedLabel();
}
