// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Wires the welcome window to the settings store, the autostart plugin
// and the `welcome_*` commands.

import { useCallback, useEffect, useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useSetting } from "../hooks/useSetting";
import { command } from "../lib/command";
import { WelcomeWindow } from "./WelcomeWindow";

export function WelcomeApp() {
  const [globalShortcut, setGlobalShortcut] = useSetting<string>("globalShortcut");
  const [storedAutomaticChecks] = useSetting<boolean | undefined>("updates.automaticChecks");

  const [launchAtLogin, setLaunchAtLoginState] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    isEnabled().then(
      (enabled) => {
        if (!cancelled) {
          setLaunchAtLoginState(enabled);
        }
      },
      () => {
        if (!cancelled) {
          setLaunchAtLoginState(false);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  // The switch shows the new state at once, as in Settings.
  const setLaunchAtLogin = useCallback((enabled: boolean) => {
    setLaunchAtLoginState(enabled);
    void (enabled ? enable() : disable()).catch(() => {});
  }, []);

  const readyToFinish = useCallback((automaticChecks: boolean) => {
    void command("welcome_ready_to_finish", { automaticChecks }).catch(() => {});
  }, []);
  const notReady = useCallback(() => {
    void command("welcome_not_ready").catch(() => {});
  }, []);
  const finish = useCallback(() => {
    void command("welcome_finish").catch(() => {});
  }, []);

  return (
    <WelcomeWindow
      globalShortcut={globalShortcut}
      setGlobalShortcut={setGlobalShortcut}
      launchAtLogin={launchAtLogin}
      setLaunchAtLogin={setLaunchAtLogin}
      storedAutomaticChecks={
        typeof storedAutomaticChecks === "boolean" ? storedAutomaticChecks : null
      }
      readyToFinish={readyToFinish}
      notReady={notReady}
      finish={finish}
    />
  );
}
