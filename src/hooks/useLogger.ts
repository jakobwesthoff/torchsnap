// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useContext } from "react";
import type { Logger } from "../lib/logger";
import { LoggerContext } from "../contexts/LoggerContext";

/**
 * Access the logger from context. Must be used within a
 * LoggerProvider.
 *
 * @throws Error if used outside a LoggerProvider.
 */
export function useLogger(): Logger {
  const logger = useContext(LoggerContext);
  if (!logger) {
    throw new Error("useLogger must be used within a LoggerProvider");
  }
  return logger;
}
