/**
 * Adapter fixture: React function component + custom hook using useContext
 * via both named and namespace imports.
 *
 * Acceptance coverage for str-zgsk — verifies that:
 *   - `createContext` resolves through the React shim (was missing, crashed
 *     at module load time before this fixture landed)
 *   - A custom hook calling `useContext` is classified as a React hook and
 *     dispatched through the react-hook adapter
 *   - A function component calling the custom hook is similarly classified
 *   - The namespace-imported form (`import * as React from "react"`) is also
 *     classified, not silently skipped
 *
 * Without the React harness in place, these functions reproduce the pickpackit
 * scan failure mode (`Cannot read properties of null (reading 'useContext')`
 * and `Invalid hook call`).
 */

import * as React from "react";
import { createContext, useContext } from "react";

interface Theme {
  dark: boolean;
  accent: string;
}

const ThemeContext = createContext<Theme>({ dark: false, accent: "blue" });

export function useThemeMode(): { mode: string; accent: string } {
  const theme = useContext(ThemeContext);
  if (theme.dark) {
    return { mode: "dark", accent: theme.accent };
  }
  return { mode: "light", accent: theme.accent };
}

export function ThemedLabel(props: { label: string }) {
  const { mode, accent } = useThemeMode();
  if (mode === "dark") {
    return <span data-mode="dark" data-accent={accent}>{props.label}</span>;
  }
  return <span data-mode="light" data-accent={accent}>{props.label}</span>;
}

export function NamespacePanel(props: { title: string }) {
  const theme = React.useContext(ThemeContext);
  if (theme.dark) {
    return <section data-ns="dark">{props.title}</section>;
  }
  return <section data-ns="light">{props.title}</section>;
}
