// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useCallback, useState } from "react";

/**
 * Open state of the mascot debug panel (`MascotDebugPanel`, dev builds
 * only). The double-click on the mascot opens it, and it stays open
 * until its own click or the launcher's reset on dismiss closes it, so
 * it never describes a draw other than the one on screen.
 */
export function useMascotDebugPanel() {
  const [open, setOpen] = useState(false);
  const show = useCallback(() => setOpen(true), []);
  const close = useCallback(() => setOpen(false), []);
  return { open, show, close };
}
