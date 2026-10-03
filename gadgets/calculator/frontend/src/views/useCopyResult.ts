// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useCallback } from "react";
import { useGadgetRuntime, useLauncher } from "@torchsnap/gadget-sdk/hooks";

/** Payload of the backend's `copy` message. */
export interface CopyRequest {
  expression: string;
  result: string;
  resultType: string;
}

/**
 * Returns a function that copies a result and closes the launcher.
 *
 * The backend's `copy` message writes the result to the clipboard
 * and records it in the history. The launcher closes only once the
 * copy succeeded; on failure it stays open, shows the error in its
 * footer and the error goes to the gadget log.
 */
export function useCopyResult(): (request: CopyRequest) => Promise<void> {
  const { dismiss, showError } = useLauncher();
  const { sendMessage, logger } = useGadgetRuntime();

  return useCallback(
    async (request: CopyRequest) => {
      try {
        await sendMessage("copy", request);
      } catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        showError(`Copying the result failed: ${detail}`);
        logger.error("copying the result failed", [["error", detail]]);
        return;
      }
      dismiss();
    },
    [dismiss, showError, sendMessage, logger],
  );
}
