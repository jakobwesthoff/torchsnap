// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Window startup
//
// Auxiliary windows (settings, devtools, welcome, update) are
// created hidden, and the backend shows one when its frontend
// emits `react-ready`. A window first runs its setup, such as
// loading the settings store, then renders its content.
//
// When the setup fails, the window renders the error instead
// and still emits `react-ready`, so the user sees what went
// wrong instead of a window that never appears.
// =========================================================

import { StrictMode, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { WindowError, WindowErrorBoundary } from "../components/WindowErrorBoundary";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { createLogger } from "./logger";

interface StartWindowOptions {
  /** Window name, logged with a startup error. */
  window: string;
  /** Setup that must finish before the content renders. */
  prepare?: () => Promise<void>;
  /** The window's content, created only after the setup succeeded. */
  app: () => ReactNode;
}

export async function startWindow({ window: name, prepare, app }: StartWindowOptions) {
  const root = createRoot(document.getElementById("root")!);

  try {
    await prepare?.();
    root.render(
      <StrictMode>
        <WindowErrorBoundary window={name}>
          <ThemeProvider>{app()}</ThemeProvider>
        </WindowErrorBoundary>
      </StrictMode>,
    );
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    createLogger("host").error(`window failed to start: ${message}`, [["window", name]]);
    root.render(
      <StrictMode>
        <ThemeProvider>
          <WindowError message={message} />
        </ThemeProvider>
      </StrictMode>,
    );
  }

  // NOTE: This fires right after `render()` returns, which schedules
  // the React tree but may not have painted yet. In practice the
  // round-trip through Tauri's event system and `run_on_main_thread`
  // adds enough latency that the first frame is composited before
  // the window becomes visible — but this is not guaranteed.
  await getCurrentWebviewWindow().emit("react-ready");
}
