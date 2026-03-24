import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  ThemeContext,
  THEME_STORAGE_KEY,
  type EffectiveTheme,
  type ThemePreference,
} from "./ThemeContext";

// =========================================================
// Helpers
// =========================================================

const DARK_MQ = "(prefers-color-scheme: dark)";

function readStoredPreference(): ThemePreference {
  const raw = localStorage.getItem(THEME_STORAGE_KEY);
  if (raw === "light" || raw === "dark" || raw === "system") {
    return raw;
  }
  return "system";
}

function resolveEffective(preference: ThemePreference): EffectiveTheme {
  if (preference === "system") {
    return window.matchMedia(DARK_MQ).matches ? "dark" : "light";
  }
  return preference;
}

function applyToDOM(effective: EffectiveTheme) {
  document.documentElement.dataset.theme = effective;
}

// =========================================================
// Provider
// =========================================================

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] =
    useState<ThemePreference>(readStoredPreference);
  const [effective, setEffective] = useState<EffectiveTheme>(() =>
    resolveEffective(preference),
  );

  // Sync the DOM attribute whenever the effective theme changes.
  useEffect(() => {
    applyToDOM(effective);
  }, [effective]);

  // Listen for OS-level theme changes so "system" mode reacts live.
  useEffect(() => {
    const mq = window.matchMedia(DARK_MQ);

    const handler = () => {
      // Only re-resolve when the user chose "system".
      if (preference === "system") {
        setEffective(resolveEffective("system"));
      }
    };

    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [preference]);

  // Sync theme preference across Tauri webview windows via the
  // browser's storage event. This fires in other browsing contexts
  // sharing the same origin — local writes are intentionally ignored
  // so consumers don't react to their own updates.
  useEffect(() => {
    const handler = (e: StorageEvent) => {
      if (e.key !== THEME_STORAGE_KEY) {
        return;
      }
      const next = (e.newValue ?? "system") as ThemePreference;
      if (next !== "light" && next !== "dark" && next !== "system") {
        return;
      }
      setPreferenceState(next);
      setEffective(resolveEffective(next));
    };

    window.addEventListener("storage", handler);
    return () => window.removeEventListener("storage", handler);
  }, []);

  const setPreference = useCallback((next: ThemePreference) => {
    localStorage.setItem(THEME_STORAGE_KEY, next);
    setPreferenceState(next);
    setEffective(resolveEffective(next));
  }, []);

  const value = useMemo(
    () => ({ preference, effective, setPreference }),
    [preference, effective, setPreference],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}
