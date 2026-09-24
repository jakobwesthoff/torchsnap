// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Wires the update window to the backend: the phase from
// `useUpdatePhase`, the buttons to the `update_*` commands, and
// closing to the window itself.

import { useMemo } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { openUrl } from "@tauri-apps/plugin-opener";
import { command } from "../lib/command";
import { useUpdatePhase } from "./useUpdatePhase";
import { UpdateWindow, type UpdateActions } from "./UpdateWindow";

export function UpdateApp() {
  const phase = useUpdatePhase();

  // `command()` logs rejections itself; the phase events carry every
  // outcome the window has to show.
  const actions = useMemo<UpdateActions>(
    () => ({
      check: () => void command("update_check").catch(() => {}),
      install: () => void command("update_install").catch(() => {}),
      skip: () => void command("update_skip").catch(() => {}),
      close: () => void getCurrentWebviewWindow().close(),
      openDownload: (url) => void openUrl(url).catch(() => {}),
    }),
    [],
  );

  return <UpdateWindow phase={phase} actions={actions} />;
}
