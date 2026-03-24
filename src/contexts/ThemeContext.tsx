import { createContext } from "react";

export type ThemePreference = "system" | "light" | "dark";
export type EffectiveTheme = "light" | "dark";

export interface ThemeContextValue {
  /** The user's stored preference (system, light, or dark). */
  preference: ThemePreference;
  /** The resolved theme currently applied to the page. */
  effective: EffectiveTheme;
  /** Update the preference — persists to localStorage and syncs the DOM. */
  setPreference: (preference: ThemePreference) => void;
}

export const THEME_STORAGE_KEY = "torchsnap_theme";

export const ThemeContext = createContext<ThemeContextValue | null>(null);
