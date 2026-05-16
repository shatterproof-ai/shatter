/**
 * Plain TypeScript React hook fixture. Hooks are often defined in `.ts`
 * modules when they do not return JSX, so execution still needs the React
 * shim even without a `.tsx` extension.
 */

import { useContext } from "react";

const ThemeContext = { _currentValue: "dark" };

function useAccentLabel(theme: string | undefined) {
  return theme === "dark" ? "contrast" : "plain";
}

export function useThemeLabel() {
  const theme = useContext(ThemeContext);
  return useAccentLabel(theme);
}
