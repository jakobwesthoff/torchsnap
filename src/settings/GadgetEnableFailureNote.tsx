// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Says that a gadget which is turned on failed to start, shows the
 * reason in the gadget's words, and offers to start it again. Shows
 * nothing while the gadget runs.
 *
 * A retry clears the failure in the backend when it starts and records
 * it again when the gadget fails once more, often within a few
 * milliseconds. The block therefore stays, with the last reason, until
 * the retry is over, and the button reads "Retrying…" for a minimum
 * time so that a retry which fails at once is still seen to happen.
 */

import { useState } from "react";
import { ExclamationTriangleIcon } from "@heroicons/react/24/outline";
import { command } from "../lib/command";
import { cn } from "../lib/cn";
import { useGadgetEnableFailure } from "./useGadgetEnableFailure";

/** How long "Retrying…" shows at least. */
const RETRY_MIN_MS = 600;

interface GadgetEnableFailureNoteProps {
  gadgetId: string;
  /** The gadget's name, as the settings list it. */
  name: string;
  /** Set where the gadget cannot be started, such as a gadget that is
   *  removed on the next restart. */
  retryDisabled?: boolean;
  className?: string;
}

export function GadgetEnableFailureNote({
  gadgetId,
  name,
  retryDisabled = false,
  className,
}: GadgetEnableFailureNoteProps) {
  const failure = useGadgetEnableFailure(gadgetId);
  // The reason shown when the retry started, kept while it runs; `null`
  // when no retry runs.
  const [retryingAfter, setRetryingAfter] = useState<string | null>(null);
  const retrying = retryingAfter !== null;

  const reason = failure ?? retryingAfter;
  if (reason === null) {
    return null;
  }

  const retry = async () => {
    setRetryingAfter(reason);
    await Promise.all([
      // A refused retry is already logged by `command()`; the block
      // then simply offers Retry again.
      command("gadget_retry_enable", { gadgetId }).catch(() => {}),
      new Promise((resolve) => setTimeout(resolve, RETRY_MIN_MS)),
    ]);
    setRetryingAfter(null);
  };

  return (
    <div
      role="alert"
      className={cn(
        "flex flex-col gap-2 rounded-md border border-red-500/30 bg-red-500/10 p-3",
        className,
      )}
    >
      <div className="flex items-center gap-2 text-red-700 dark:text-red-400">
        <ExclamationTriangleIcon aria-hidden className="h-4 w-4 shrink-0" />
        <span className="flex-1 text-sm font-medium">{name} failed to start</span>
        {/* On the red tint the button has its own surface and a border
            of about 3:1 against the tint, so it reads as a button in
            both themes. "Retrying…" is a state to read, so the disabled
            button stays at 70 %. */}
        <button
          type="button"
          onClick={() => void retry()}
          disabled={retrying || retryDisabled}
          className="shrink-0 rounded-md border border-red-600/75 bg-white/70 px-2.5 py-1 text-xs font-medium text-red-700 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent enabled:hover:border-red-600 enabled:hover:bg-white disabled:cursor-not-allowed disabled:opacity-70 dark:border-red-400/65 dark:bg-white/5 dark:text-red-300 dark:enabled:hover:border-red-400/90 dark:enabled:hover:bg-white/10"
        >
          {retrying ? "Retrying…" : "Retry"}
        </button>
      </div>
      <p className="rounded bg-black/5 px-2 py-1.5 font-mono text-xs break-words whitespace-pre-wrap text-text-secondary select-text dark:bg-black/30">
        {reason}
      </p>
    </div>
  );
}
