// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Says that a gadget which is turned on failed to start, and why, next
 * to its switch. Shows nothing while the gadget runs.
 */

import { useGadgetEnableFailure } from "./useGadgetEnableFailure";

export function GadgetEnableFailureNote({ gadgetId }: { gadgetId: string }) {
  const failure = useGadgetEnableFailure(gadgetId);
  if (failure === null) {
    return null;
  }
  return (
    <p role="alert" className="text-xs text-red-500">
      Failed to start, so it does not work. Turn it off and on again to retry. ({failure})
    </p>
  );
}
