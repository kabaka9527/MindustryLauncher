import { useCallback, useEffect, useState } from "react";
import type { Theme } from "../types";

const STORAGE_KEY = "mindustry-launcher-theme";

function readStoredTheme(): Theme {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "system" || raw === "light" || raw === "dark") {
      return raw;
    }
  } catch {
    // localStorage unavailable
  }
  return "system";
}

function resolveEffectiveTheme(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return theme;
}

/**
 * Manages the app's colour theme.
 *
 * - Persists the user's choice (system / light / dark) in localStorage.
 * - Applies `data-theme="light|dark"` to `<html>` so CSS can react.
 * - When the user picks "system", listens for OS-level changes and
 *   updates the attribute in real time.
 */
export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(readStoredTheme);

  // Sync the DOM attribute whenever the effective theme changes
  useEffect(() => {
    const effective = resolveEffectiveTheme(theme);
    document.documentElement.setAttribute("data-theme", effective);

    if (theme !== "system") {
      return;
    }

    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (event: MediaQueryListEvent) => {
      document.documentElement.setAttribute(
        "data-theme",
        event.matches ? "dark" : "light",
      );
    };
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // localStorage unavailable
    }
  }, []);

  // Derived boolean for convenience
  const isDark = resolveEffectiveTheme(theme) === "dark";

  return { theme, isDark, setTheme } as const;
}