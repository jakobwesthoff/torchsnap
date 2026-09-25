// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Start section
//
// The backend keeps at most one section the Settings window should
// show, for example after "Restart now" or when a gadget returns
// `PostAction::OpenSettings`. It hands the section out once through
// `take_settings_start_section`. A new window takes it on mount; an
// open window takes it when the backend emits
// `SETTINGS_START_SECTION`.
// =========================================================

import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { command } from "../lib/command";

/** Emitted to the Settings window when a section is waiting to be taken. */
export const SETTINGS_START_SECTION = "settings-start-section";

export function useStartSection(onSection: (section: string) => void): void {
  // The latest callback, read when a section arrives, so the effect
  // does not re-subscribe on every render.
  const onSectionRef = useRef(onSection);
  useEffect(() => {
    onSectionRef.current = onSection;
  }, [onSection]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    // A taken section is gone from the backend, so a result that
    // arrives after an unmount is still applied: a development
    // remount would otherwise receive nothing.
    const take = () =>
      command("take_settings_start_section").then(
        (section) => {
          if (section) {
            onSectionRef.current(section);
          }
        },
        () => {},
      );

    void take();
    void listen(SETTINGS_START_SECTION, () => void take()).then((u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
