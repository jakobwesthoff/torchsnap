import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSetting } from "../hooks/useSetting";
import { SETTINGS_DEFAULTS } from "../settingsDefaults";

export function SettingsPanel() {
  const [globalShortcut, , shortcutReady] = useSetting(
    "globalShortcut",
    SETTINGS_DEFAULTS.globalShortcut,
  );

  const loading = !shortcutReady;

  // Close the settings window on Escape, unless an input is focused
  // (Escape in an input should blur it, not close the window).
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const active = document.activeElement;
        if (
          active instanceof HTMLInputElement ||
          active instanceof HTMLTextAreaElement
        ) {
          active.blur();
          return;
        }
        getCurrentWindow().hide();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div
      className={`min-h-screen font-sans antialiased bg-surface text-text-primary ${
        loading ? "opacity-0" : "opacity-100 transition-opacity duration-300"
      }`}
    >
      {/* Drag region — macOS overlay titlebar */}
      <div data-tauri-drag-region className="h-12 w-full select-none" />

      {/* Content */}
      <div className="px-6 pb-6">
        <h2 className="text-lg font-semibold mb-4">Settings</h2>

        {/* Shortcut section placeholder */}
        <div className="rounded-xl border border-border p-4">
          <label className="text-sm text-text-secondary">Global Shortcut</label>
          <div className="mt-1 text-sm text-text-primary font-mono">
            {globalShortcut}
          </div>
        </div>
      </div>
    </div>
  );
}
